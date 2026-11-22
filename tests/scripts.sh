#!/bin/sh
# install.sh replaces a 0.x Python install without touching user data;
# uninstall.sh keeps data unless --purge. Usage: tests/scripts.sh PATH/TO/niruflow
set -eu
BIN=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
ROOT=$(cd "$(dirname "$0")/.." && pwd)
SB=$(mktemp -d)
trap 'rm -rf "$SB"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }
H=$SB/home
mkdir -p "$H/.local/share/niruflow/versions/0.6.1/venv/bin" "$H/.local/bin" "$H/.config/niruflow" \
    "$H/.local/state/niruflow/quarantine" "$H/.config/systemd/user"
echo py > "$H/.local/share/niruflow/versions/0.6.1/venv/bin/niruflow"
ln -s "$H/.local/share/niruflow/versions/0.6.1" "$H/.local/share/niruflow/current"
ln -s "$H/.local/share/niruflow/current/venv/bin/niruflow" "$H/.local/bin/niruflow"
echo '{"rules":[]}' > "$H/.config/niruflow/config.json"
echo q > "$H/.local/state/niruflow/quarantine/a.txt"
printf '[Service]\nExecStart=%s/.local/share/niruflow/versions/0.6.1/venv/bin/niruflow run-enabled\n' "$H" > "$H/.config/systemd/user/niruflow.service"
# A release-like folder: the binary beside the scripts.
REL=$SB/rel
mkdir -p "$REL/assets"
cp "$ROOT/install.sh" "$ROOT/uninstall.sh" "$ROOT/Cargo.toml" "$REL/"
cp "$ROOT/assets/niruflow.svg" "$ROOT/assets/niruflow.desktop" "$REL/assets/"
cp "$BIN" "$REL/niruflow"
(cd "$REL" && sha256sum niruflow > SHA256SUMS)
run() { env -i HOME="$H" PATH="/usr/bin:/bin" "$@"; }
run sh "$REL/install.sh" --prebuilt >/dev/null || fail "install"
[ -f "$H/.local/bin/niruflow" ] && [ ! -L "$H/.local/bin/niruflow" ] || fail "launcher not replaced"
[ ! -e "$H/.local/share/niruflow" ] || fail "old venv kept"
[ -f "$H/.config/niruflow/config.json" ] && [ -f "$H/.local/state/niruflow/quarantine/a.txt" ] || fail "user data touched"
grep -q "^ExecStart=$H/.local/bin/niruflow run-enabled$" "$H/.config/systemd/user/niruflow.service" || fail "scheduler unit not updated"
grep -q "^Exec=$H/.local/bin/niruflow desktop$" "$H/.local/share/applications/niruflow.desktop" || fail "desktop entry"
run "$H/.local/bin/niruflow" --version | grep -q '^NIRUFLOW ' || fail "installed binary"
# A tampered binary is refused.
echo x >> "$REL/niruflow"
run sh "$REL/install.sh" --prebuilt >/dev/null 2>&1 && fail "tampered binary installed"
run sh "$REL/uninstall.sh" >/dev/null || fail "uninstall"
[ ! -e "$H/.local/bin/niruflow" ] || fail "binary kept"
[ -f "$H/.config/niruflow/config.json" ] && [ -f "$H/.local/state/niruflow/quarantine/a.txt" ] || fail "uninstall removed data"
run sh "$REL/uninstall.sh" --purge >/dev/null || fail "purge"
[ ! -e "$H/.config/niruflow" ] && [ ! -e "$H/.local/state/niruflow" ] || fail "purge kept data"
echo "PASS"
