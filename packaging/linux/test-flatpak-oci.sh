#!/usr/bin/env bash
# Exercise OCI routing and authentication without building, installing or publishing anything.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FIXTURE="$(mktemp -d "${TMPDIR:-/tmp}/pdfcraft-flatpak-oci.XXXXXXXX")"
trap 'rm -rf "$FIXTURE"' EXIT
mkdir -p "$FIXTURE/packaging/linux" "$FIXTURE/bin" "$FIXTURE/out"
cp "$HERE/flatpak-oci.sh" "$FIXTURE/packaging/linux/"
cat > "$FIXTURE/packaging/env.sh" <<'EOF'
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION=1.0.2
DIST="$ROOT/out"
export ROOT VERSION DIST
EOF
cat > "$FIXTURE/packaging/linux/flatpak-bundle.sh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[[ "$1" == --oci && "$2" == --no-test ]]
mkdir -p "$DIST/pdfcraft-$VERSION-linux-x86_64-oci"
EOF
cat > "$FIXTURE/bin/uname" <<'EOF'
#!/usr/bin/env bash
echo x86_64
EOF
cat > "$FIXTURE/bin/skopeo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >> "$OCI_TEST_LOG"
if [[ "$1" == login ]]; then
  [[ "$*" == *--password-stdin* && "$*" == *--authfile* ]]
  [[ "$(cat "$3")" == '{"auths":{}}' ]]
  [[ "$(cat)" == synthetic-test-token ]]
else
  [[ "$1" == copy && "$*" == *--dest-authfile* ]]
fi
EOF
chmod +x "$FIXTURE/bin/"* "$FIXTURE/packaging/linux/flatpak-bundle.sh"
export PATH="$FIXTURE/bin:$PATH" OCI_TEST_LOG="$FIXTURE/commands.log"
unset REGISTRY GITHUB_REPOSITORY TAG GITHUB_TOKEN
bash "$FIXTURE/packaging/linux/flatpak-oci.sh" > "$FIXTURE/build.log"
[[ ! -e "$OCI_TEST_LOG" ]]
export GITHUB_TOKEN=synthetic-test-token GITHUB_ACTOR=test-user GITHUB_REPOSITORY=gh0sted5456-us/pdfcraft
bash "$FIXTURE/packaging/linux/flatpak-oci.sh" --push > "$FIXTURE/push.log"
[[ "$(wc -l < "$OCI_TEST_LOG")" -eq 3 ]]
grep -q 'docker://ghcr.io/gh0sted5456-us/pdfcraft:1.0.2-x86_64' "$OCI_TEST_LOG"
grep -q 'docker://ghcr.io/gh0sted5456-us/pdfcraft:stable-x86_64' "$OCI_TEST_LOG"
if grep -q synthetic-test-token "$OCI_TEST_LOG" "$FIXTURE/push.log"; then
  echo 'Token leaked into command arguments or output' >&2
  exit 1
fi
[[ -z "$(find "$FIXTURE/out" -name 'ghcr-auth.*' -print -quit)" ]]
echo 'Flatpak OCI regression: build-only default, fork registry, stdin authentication and temporary credential cleanup passed'
