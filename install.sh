#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
command -v python3 >/dev/null || { echo "Python 3 is required" >&2; exit 1; }
VERSION="0.6.0"
BASE="${XDG_DATA_HOME:-$HOME/.local/share}/niruflow"
STAGING="$BASE/versions/$VERSION"
mkdir -p "$BASE/versions" "$HOME/.local/bin" "${XDG_DATA_HOME:-$HOME/.local/share}/applications" "${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/scalable/apps"
# Install into a dedicated version directory. Never overwrite user data or an older release.
if [[ ! -x "$STAGING/venv/bin/python" ]]; then
    python3 -m venv "$STAGING/venv"
fi
"$STAGING/venv/bin/python" -m pip install --disable-pip-version-check '.[desktop]'
# Only compile first-party source. PySide6 ships non-Python Jinja templates ending in .py.
"$STAGING/venv/bin/python" -m compileall -q niruflow
"$STAGING/venv/bin/python" -c 'from niruflow.core.version import VERSION; assert VERSION == "0.6.0"; from niruflow.desktop.app import launch'
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
