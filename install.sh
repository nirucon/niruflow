#!/bin/sh
# NIRUFLOW installer.
#
#   ./install.sh             install the prebuilt release binary (verified by SHA-256)
#   ./install.sh --build     build from this checkout with cargo instead
#   ./install.sh --prebuilt  use ./niruflow beside this script (a release download)
#
# Installs ~/.local/bin/niruflow, the desktop entry and the icon. No root.
# An older Python install (0.x: a venv under ~/.local/share/niruflow and a
# launcher symlink) is replaced; rules, history and quarantine in
# ~/.config/niruflow and ~/.local/state/niruflow are never touched.
set -eu
cd "$(dirname "$0")"

PREFIX="${NIRUFLOW_PREFIX:-$HOME/.local}"
BINDIR="$PREFIX/bin"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
APPDIR="$DATA/applications"
ICONDIR="$DATA/icons/hicolor/scalable/apps"
CONFIG="${XDG_CONFIG_HOME:-$HOME/.config}"
MODE=auto

for arg in "$@"; do
    case "$arg" in
        --build) MODE=build ;;
        --prebuilt) MODE=prebuilt ;;
        -h|--help)
            printf '%s\n' "Usage: ./install.sh [--build|--prebuilt]"
            exit 0
            ;;
        *) printf 'niruflow install: unknown option %s\n' "$arg" >&2; exit 2 ;;
    esac
done

say() { printf '%s\n' "$*"; }
die() { say "niruflow install: $*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1; }

[ "$(id -u)" != 0 ] || die 'run ./install.sh as your desktop user, not as root'

VERSION="${NIRUFLOW_VERSION:-}"
if [ -z "$VERSION" ] && [ -f Cargo.toml ]; then
    VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
fi
[ -n "$VERSION" ] || die 'cannot determine the NIRUFLOW version (set NIRUFLOW_VERSION)'

TMP=""
trap 'rm -rf "$TMP"' EXIT
if [ "$MODE" = build ]; then
    need cargo || die 'Rust/cargo is required for --build'
    say 'Building NIRUFLOW…'
    cargo build --release --locked
    BIN_SRC=target/release/niruflow
elif [ "$MODE" = prebuilt ] || [ -x ./niruflow ]; then
    [ -x ./niruflow ] || die './niruflow not found beside install.sh'
    BIN_SRC=./niruflow
else
    [ "$(uname -m)" = x86_64 ] || die "no prebuilt release for $(uname -m); use ./install.sh --build"
    need curl || need wget || die 'curl or wget is required to download the prebuilt binary'
    need sha256sum || die 'sha256sum is required to verify the release binary'
    TMP=$(mktemp -d)
    BASE="https://github.com/nirucon/niruflow/releases/download/v${VERSION}"
    say "Downloading prebuilt NIRUFLOW ${VERSION}…"
    for f in niruflow SHA256SUMS; do
        if need curl; then
            curl -fsSL --retry 3 -o "$TMP/$f" "$BASE/$f" || die "could not download $f; nothing was changed"
        else
            wget -qO "$TMP/$f" "$BASE/$f" || die "could not download $f; nothing was changed"
        fi
    done
    BIN_SRC="$TMP/niruflow"
    SUMS="$TMP/SHA256SUMS"
fi
if [ "$MODE" != build ]; then
    SUMS="${SUMS:-SHA256SUMS}"
    if [ -f "$SUMS" ]; then
        need sha256sum || die 'sha256sum is required to verify the release binary'
        expected=$(awk '$2 == "niruflow" || $2 == "*niruflow" { print $1; exit }' "$SUMS")
        actual=$(sha256sum "$BIN_SRC" | awk '{print $1}')
        [ -n "$expected" ] && [ "$expected" = "$actual" ] || die 'the NIRUFLOW binary failed SHA-256 verification; nothing was changed'
        say 'SHA-256 verification passed.'
    elif [ -n "$TMP" ]; then
        die 'release checksums missing'
    fi
fi
chmod +x "$BIN_SRC"
got=$("$BIN_SRC" --version 2>/dev/null | awk '{print $NF}')
[ "$got" = "$VERSION" ] || die "the binary reports version '$got', expected $VERSION"

install -d -m 0755 "$BINDIR" "$APPDIR" "$ICONDIR"
# 0.x left ~/.local/bin/niruflow as a symlink into its venv: replace the
# link itself, never write through it.
rm -f "$BINDIR/niruflow.new"
install -m 0755 "$BIN_SRC" "$BINDIR/niruflow.new"
mv -f "$BINDIR/niruflow.new" "$BINDIR/niruflow"
install -m 0644 assets/niruflow.svg "$ICONDIR/niruflow.svg"
sed "s|^Exec=niruflow desktop$|Exec=$BINDIR/niruflow desktop|" assets/niruflow.desktop > "$APPDIR/niruflow.desktop"
chmod 0644 "$APPDIR/niruflow.desktop"

# The 0.x scheduler unit runs the resolved venv path; point it at the binary.
UNIT="$CONFIG/systemd/user/niruflow.service"
if [ -f "$UNIT" ] && grep -q '^ExecStart=.*/niruflow/versions/' "$UNIT"; then
    sed -i "s|^ExecStart=.*|ExecStart=$BINDIR/niruflow run-enabled|" "$UNIT"
    if need systemctl; then systemctl --user daemon-reload >/dev/null 2>&1 || :; fi
    say 'Scheduler unit now runs the new binary.'
fi

# The Python versions (venvs only; no user data lives there).
if [ -d "$DATA/niruflow/versions" ] || [ -L "$DATA/niruflow/current" ]; then
    rm -rf "$DATA/niruflow/versions" "$DATA/niruflow/current" "$DATA/niruflow/current.next"
    rmdir "$DATA/niruflow" 2>/dev/null || :
    say 'Removed the old Python installation (rules, history and quarantine kept).'
fi

if need update-desktop-database; then update-desktop-database "$APPDIR" >/dev/null 2>&1 || :; fi
if need gtk-update-icon-cache; then gtk-update-icon-cache -q "$DATA/icons/hicolor" >/dev/null 2>&1 || :; fi

say "NIRUFLOW $VERSION installed at $BINDIR/niruflow"
say 'Configuration, history and quarantine have been preserved.'
