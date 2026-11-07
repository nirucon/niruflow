#!/usr/bin/env bash
# Remove NIRUFLOW: the scheduler timer, the launcher, the desktop entry, the
# icon and the installed versions. Rules, history and quarantined files in
# ~/.config/niruflow and ~/.local/state/niruflow are kept; --purge removes them
# too. Check ~/.local/state/niruflow/quarantine/ before purging.
set -euo pipefail
purge=0
for arg in "$@"; do
    case "$arg" in
        --purge) purge=1 ;;
        -h|--help) echo "usage: ./uninstall.sh [--purge]"; exit 0 ;;
        *) echo "unknown option: $arg (try --help)" >&2; exit 2 ;;
    esac
done
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
CONFIG="${XDG_CONFIG_HOME:-$HOME/.config}"
STATE="${XDG_STATE_HOME:-$HOME/.local/state}"
if command -v systemctl >/dev/null 2>&1 && [[ -f "$CONFIG/systemd/user/niruflow.timer" ]]; then
    systemctl --user disable --now niruflow.timer >/dev/null 2>&1 || :
fi
rm -f "$CONFIG/systemd/user/niruflow.service" "$CONFIG/systemd/user/niruflow.timer"
command -v systemctl >/dev/null 2>&1 && systemctl --user daemon-reload >/dev/null 2>&1 || :
rm -f "$HOME/.local/bin/niruflow" "$DATA/applications/niruflow.desktop" "$DATA/icons/hicolor/scalable/apps/niruflow.svg"
rm -rf "$DATA/niruflow"
if [[ $purge == 1 ]]; then
    rm -rf "$CONFIG/niruflow" "$STATE/niruflow"
    echo "NIRUFLOW removed, including rules, history and quarantine."
else
    echo "NIRUFLOW removed. Kept rules and history:"
    for d in "$CONFIG/niruflow" "$STATE/niruflow"; do [[ -e $d ]] && echo "  $d"; done
    echo "Check $STATE/niruflow/quarantine/ for files you want back; ./uninstall.sh --purge deletes it all."
fi
