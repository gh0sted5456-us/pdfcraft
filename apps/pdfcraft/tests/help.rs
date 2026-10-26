//! `--help`, `-h` and `--version` print and exit without opening a window (#4076), so a script that
//! calls them returns.

use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_pdfcraft");

/// Run the app with `args`, failing if it is still running after 60 s (it opened its window).
fn run<A: AsRef<std::ffi::OsStr> + std::fmt::Debug>(args: &[A], stdout: Stdio) -> Output {
    let mut child = Command::new(BIN).args(args).stdin(Stdio::null()).stdout(stdout).stderr(Stdio::piped()).spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("pdfcraft {args:?} did not exit: it started the app instead");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    child.wait_with_output().unwrap()
}

#[test]
fn help_prints_usage_and_exits() {
    for flag in ["--help", "-h"] {
        let out = run(&[flag], Stdio::piped());
        assert!(out.status.success(), "{flag}: {:?} {}", out.status, String::from_utf8_lossy(&out.stderr));
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.starts_with("Usage: pdfcraft [options] [files…]"), "{flag}: {stdout}");
        assert!(stdout.contains("pdfcraft-cli"), "{flag}: points scripts to the CLI: {stdout}");
    }
    // Wherever it is on the command line, and with files named.
    let out = run(&["missing.pdf", "--help"], Stdio::piped());
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("Usage:"));
}

#[test]
fn version_prints_the_version_and_exits() {
    let out = run(&["--version"], Stdio::piped());
    assert!(out.status.success(), "{:?} {}", out.status, String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("pdfcraft "), "{}", String::from_utf8_lossy(&out.stdout));
}

/// An argument that isn't UTF-8 (a Latin-1 file name, as a Linux file manager passes it) is skipped
/// with a warning instead of panicking in `std::env::args` (exit 101), and the ones after it keep
/// their place: `--control` gets no value rather than taking `--version` as its file.
#[cfg(any(unix, windows))]
#[test]
fn an_argument_that_is_not_utf8_is_skipped_not_a_crash() {
    use std::ffi::OsStr;
    #[cfg(unix)]
    use std::os::unix::ffi::OsStrExt;
    #[cfg(windows)]
    use std::os::windows::ffi::OsStringExt;
    #[cfg(unix)]
    let latin1 = OsStr::from_bytes(b"caf\xe9.pdf");
    #[cfg(windows)]
    let invalid = std::ffi::OsString::from_wide(&[0x63, 0x61, 0x66, 0xd800, 0x2e, 0x70, 0x64, 0x66]);
    #[cfg(windows)]
    let latin1 = invalid.as_os_str();
    for args in [vec![latin1, OsStr::new("--version")], vec![OsStr::new("--control"), latin1, OsStr::new("--version")]] {
        let out = run(&args, Stdio::piped());
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{args:?}: {:?} {stderr}", out.status);
        assert!(String::from_utf8_lossy(&out.stdout).starts_with("pdfcraft "), "{args:?}");
        assert!(stderr.contains("caf\u{FFFD}.pdf") && stderr.contains("not UTF-8"), "{args:?}: says what it skipped: {stderr}");
    }
}

/// A standard output that can't be written (here a full device) ends the run normally: `println!`
/// would panic and exit 101.
#[cfg(target_os = "linux")]
#[test]
fn help_and_version_survive_an_unwritable_stdout() {
    for flag in ["--help", "--version"] {
        let full = std::fs::OpenOptions::new().write(true).open("/dev/full").unwrap();
        let out = run(&[flag], Stdio::from(full));
        assert!(out.status.success(), "{flag}: {:?} {}", out.status, String::from_utf8_lossy(&out.stderr));
    }
}

#[cfg(windows)]
#[test]
fn refreshing_thumbnail_cache_exits_without_a_window() {
    for args in [vec!["--refresh-thumbnails"], vec!["--refresh-thumbnails", "compatibility", "--version"]] {
        let out = run(&args, Stdio::piped());
        assert!(out.status.success(), "{:?}: {}", args, String::from_utf8_lossy(&out.stderr));
        assert!(out.stdout.is_empty(), "handled the refresh flag, not the compatibility --version");
    }
}
