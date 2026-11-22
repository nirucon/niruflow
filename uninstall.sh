#!/bin/sh
# Remove NIRUFLOW: the scheduler timer, the binary, the desktop entry, the
# icon and any leftover 0.x Python installation. Rules, history and
# quarantined files in ~/.config/niruflow and ~/.local/state/niruflow are
# kept; --purge removes them too. Check ~/.local/state/niruflow/quarantine/
# before purging.
set -eu
purge=0
for arg in "$@"; do
    case "$arg" in
        --purge) purge=1 ;;
        -h|--help) echo "usage: ./uninstall.sh [--purge]"; exit 0 ;;
        *) echo "unknown option: $arg (try --help)" >&2; exit 2 ;;
    esac
done
PREFIX="${NIRUFLOW_PREFIX:-$HOME/.local}"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
CONFIG="${XDG_CONFIG_HOME:-$HOME/.config}"
STATE="${XDG_STATE_HOME:-$HOME/.local/state}"
if command -v systemctl >/dev/null 2>&1 && [ -f "$CONFIG/systemd/user/niruflow.timer" ]; then
    systemctl --user disable --now niruflow.timer >/dev/null 2>&1 || :
fi
rm -f "$CONFIG/systemd/user/niruflow.service" "$CONFIG/systemd/user/niruflow.timer"
if command -v systemctl >/dev/null 2>&1; then systemctl --user daemon-reload >/dev/null 2>&1 || :; fi
rm -f "$PREFIX/bin/niruflow" "$DATA/applications/niruflow.desktop" "$DATA/icons/hicolor/scalable/apps/niruflow.svg"
rm -rf "$DATA/niruflow"
if [ "$purge" = 1 ]; then
    rm -rf "$CONFIG/niruflow" "$STATE/niruflow"
    echo "NIRUFLOW removed, including rules, history and quarantine."
else
    echo "NIRUFLOW removed. Kept rules and history:"
    for d in "$CONFIG/niruflow" "$STATE/niruflow"; do [ -e "$d" ] && echo "  $d"; done
    echo "Check $STATE/niruflow/quarantine/ for files you want back; ./uninstall.sh --purge deletes it all."
fi
