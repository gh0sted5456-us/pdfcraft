#!/usr/bin/env python3
"""Exercise the shipped print-packaging scripts without a printer or cross compiler."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
BASH = shutil.which("bash") or str(Path(os.environ.get("ProgramFiles", "C:/Program Files")) / "Git/bin/bash.exe")


def shell(script, config, server=None, target=None):
    env = os.environ.copy()
    env.pop("CUPS_SERVER", None)
    env.pop("CARGO_BUILD_TARGET", None)
    if server is not None:
        env["CUPS_SERVER"] = server
    if target is not None:
        env["CARGO_BUILD_TARGET"] = target
    return subprocess.run([BASH, "-c", script, "lp", str(config)], env=env, capture_output=True, text=True, timeout=10)


class PrintPackaging(unittest.TestCase):
    def test_flatpak_socket_selection(self):
        scripts = []
        for name in ("ai.storyteller.pdfcraft.yml", "ai.storyteller.pdfcraft.bundle.yml"):
            text = (ROOT / "packaging/linux/flatpak" / name).read_text(encoding="utf-8")
            block = text.split("dest-filename: cups-socket.sh\n", 1)[1].split("  - name: pdfcraft", 1)[0]
            commands = []
            for line in block.split("        commands:\n", 1)[1].splitlines():
                if not line.strip():
                    continue
                self.assertTrue(line.startswith("          - '") and line.endswith("'"), line)
                commands.append(line[len("          - '"):-1].replace("''", "'"))
            scripts.append(commands)
        self.assertEqual(scripts[0], scripts[1], "both Flatpak variants must route printers identically")
        commands = scripts[0]
        self.assertEqual(commands[0], 'conf="$HOME/.cups/client.conf"')
        self.assertEqual(commands[-1], 'exec "/app/libexec/cups/${0##*/}" "$@"')
        # Substitute only the fixture location and final exec; routing runs exactly as shipped.
        script = '\n'.join(['conf="$1"', *commands[1:-1], 'printf "server:%s\\n" "${CUPS_SERVER-}"'])
        cases = [
            (None, None, ""),
            ("ServerName office.example:631\n", None, ""),
            ("ServerName /run/office.sock\n", None, "/var/run/cups/cups.sock"),
            ("  ServerName   /run/office.sock\n", None, "/var/run/cups/cups.sock"),
            ("# ServerName /run/other.sock\nServerName /run/office.sock\n", None, "/var/run/cups/cups.sock"),
            ("ServerName /run/office.sock\nServerName /run/office.sock\n", None, "/var/run/cups/cups.sock"),
            ("servername /run/office.sock\n", None, None),
            ("ServerName\t/run/office.sock\n", None, None),
            ("ServerName /run/first.sock\nServerName /run/last.sock\n", None, None),
            ("ServerName /run/first.sock\nServerName /var/run/cups/cups.sock\n", None, None),
            ("ServerName /run/office.sock\n", "remote.example:631", "remote.example:631"),
            ("ServerName /run/office.sock\n", "/run/override.sock", "/run/override.sock"),
        ]
        with tempfile.TemporaryDirectory(prefix="pdfcraft-cups-routing-") as directory:
            config = Path(directory) / "client.conf"
            for text, server, expected in cases:
                with self.subTest(config=text, server=server):
                    if text is None:
                        if config.exists():
                            config.unlink()
                    else:
                        config.write_text(text, encoding="utf-8")
                    result = shell(script, config.as_posix(), server=server)
                    if expected is None:
                        self.assertNotEqual(result.returncode, 0, result.stdout)
                        self.assertNotIn("server:", result.stdout)
                    else:
                        self.assertEqual(result.returncode, 0, result.stderr)
                        self.assertEqual(result.stdout.strip(), "server:" + expected)

    def test_freebsd_build_target(self):
        source = (ROOT / "packaging/freebsd/package.sh").read_text(encoding="utf-8")
        header = source[source.index("release_arch() {"):source.index('\nWORK=')]
        # Run the real argument/architecture logic, stopping before package files are touched.
        script = r"""
ROOT="$1"
VERSION=1.0.0
SKIP_BUILD=0
CARGO_TARGET_DIR=/fixture-target
cargo() { printf 'arg:%s\n' "$@"; }
rustc() { printf 'host: x86_64-unknown-freebsd\n'; }
uname() { if [ "${1:-}" = -s ]; then echo FreeBSD; else echo x86_64; fi; }
""" + header + '\nprintf "arch:%s\\nbin:%s\\n" "$ARCH" "$BIN"\n'
        with tempfile.TemporaryDirectory(prefix="pdfcraft-target-") as directory:
            for target, effective, arch in [
                (None, None, "x86_64"),
                ("aarch64-unknown-freebsd", "aarch64-unknown-freebsd", "aarch64"),
                ("x86_64-unknown-freebsd", "x86_64-unknown-freebsd", "x86_64"),
                ("host-tuple", "x86_64-unknown-freebsd", "x86_64"),
            ]:
                with self.subTest(target=target):
                    result = shell(script, Path(directory).as_posix(), target=target)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    args = [line[4:] for line in result.stdout.splitlines() if line.startswith("arg:")]
                    expected = ["build", "--release", "--locked"]
                    if effective:
                        expected += ["--target", effective]
                    expected += ["-p", "pdfcraft", "-p", "pdfcraft-cli"]
                    self.assertEqual(args, expected)
                    self.assertIn("arch:" + arch, result.stdout.splitlines())
                    self.assertIn("bin:/fixture-target/" + (effective + "/" if effective else "") + "release", result.stdout.splitlines())
            rejected = shell(script, Path(directory).as_posix(), target="x86_64-unknown-linux-gnu")
            self.assertNotEqual(rejected.returncode, 0)
            self.assertNotIn("arg:", rejected.stdout)


if __name__ == "__main__":
    unittest.main()
