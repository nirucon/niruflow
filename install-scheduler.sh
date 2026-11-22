#!/bin/sh
# The optional hourly systemd user timer: runs `niruflow run-enabled`
# (enabled rules marked automatic; synced folders are always skipped).
set -eu
command -v systemctl >/dev/null || { echo "systemd not found; use cron/runit scheduling manually"; exit 1; }
mkdir -p "$HOME/.config/systemd/user"
cat > "$HOME/.config/systemd/user/niruflow.service" <<UNIT
[Unit]
Description=NIRUFLOW scheduled rules
[Service]
Type=oneshot
ExecStart=$HOME/.local/bin/niruflow run-enabled
NoNewPrivileges=true
UNIT
cat > "$HOME/.config/systemd/user/niruflow.timer" <<'UNIT'
[Unit]
Description=Run NIRUFLOW every hour
[Timer]
OnCalendar=hourly
Persistent=true
RandomizedDelaySec=3min
[Install]
WantedBy=timers.target
UNIT
systemctl --user daemon-reload
systemctl --user enable --now niruflow.timer
systemctl --user list-timers niruflow.timer
