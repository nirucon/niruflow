#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
command -v python3 >/dev/null || { echo "Python 3 is required" >&2; exit 1; }
VERSION="0.6.1"
BASE="${XDG_DATA_HOME:-$HOME/.local/share}/niruflow"
STAGING="$BASE/versions/$VERSION"
mkdir -p "$BASE/versions" "$HOME/.local/bin" "${XDG_DATA_HOME:-$HOME/.local/share}/applications" "${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/scalable/apps"
# Debian ships venv's ensurepip in python3-venv; Arch and Void include it with
# Python. Without it `python3 -m venv` leaves a venv that has no pip.
ensure_venv_support() {
    python3 -c 'import ensurepip' >/dev/null 2>&1 && return 0
    command -v apt-get >/dev/null 2>&1 || { echo "Python venv support (ensurepip) is missing; install it with your package manager." >&2; exit 1; }
    if [[ ${NIRUFLOW_ASSUME_YES:-${NIRUX_ASSUME_YES:-0}} != 1 ]]; then
        [[ -t 0 ]] || { echo "python3-venv is missing. Install it with: sudo apt-get install python3-venv (or rerun with NIRUFLOW_ASSUME_YES=1)." >&2; exit 1; }
        read -r -p "python3-venv is required. Install it with sudo apt-get? [Y/n] " answer || answer=n
        [[ $answer == [nN]* ]] && { echo "python3-venv is required." >&2; exit 1; }
    fi
    sudo env DEBIAN_FRONTEND=noninteractive apt-get install -y python3-venv
}
# Install into a dedicated version directory. Never overwrite user data or an older release.
# A venv whose creation failed half-way has a python but no pip; rebuild it.
if [[ -x "$STAGING/venv/bin/python" ]] && ! "$STAGING/venv/bin/python" -m pip --version >/dev/null 2>&1; then
    rm -rf "$STAGING/venv"
fi
if [[ ! -x "$STAGING/venv/bin/python" ]]; then
    ensure_venv_support
    python3 -m venv "$STAGING/venv"
fi
"$STAGING/venv/bin/python" -m pip install --disable-pip-version-check '.[desktop]'
# Only compile first-party source. PySide6 ships non-Python Jinja templates ending in .py.
"$STAGING/venv/bin/python" -m compileall -q niruflow
"$STAGING/venv/bin/python" -c 'from niruflow.core.version import VERSION; assert VERSION == "0.6.1"; from niruflow.desktop.app import launch'
# Atomic symlink replacement; preserve the legacy venv and all user data.
ln -sfn "$STAGING" "$BASE/current.next"
mv -Tf "$BASE/current.next" "$BASE/current"
ln -sfn "$BASE/current/venv/bin/niruflow" "$HOME/.local/bin/niruflow"
cp niruflow.svg "${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/scalable/apps/niruflow.svg"
cat > "${XDG_DATA_HOME:-$HOME/.local/share}/applications/niruflow.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=NIRUFLOW
Comment=Local automation and maintenance
Exec=$HOME/.local/bin/niruflow desktop
Icon=niruflow
Terminal=false
Categories=Utility;System;
EOF
printf 'NIRUFLOW %s installed. Run: %s\n' "$VERSION" "$HOME/.local/bin/niruflow"
printf 'Older installations and user configuration/history have been preserved.\n'
