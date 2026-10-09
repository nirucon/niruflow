#!/usr/bin/env bash
set -euo pipefail
command -v systemctl >/dev/null || { echo "systemd not found; use cron/runit scheduling manually"; exit 1; }
mkdir -p "$HOME/.config/systemd/user"
cat > "$HOME/.config/systemd/user/niruflow.service" <<EOF
[Unit]
Description=NIRUFLOW scheduled rules
[Service]
Type=oneshot
ExecStart=$HOME/.local/bin/niruflow run-enabled
NoNewPrivileges=true
EOF
cat > "$HOME/.config/systemd/user/niruflow.timer" <<'EOF'
[Unit]
Description=Run NIRUFLOW every hour
[Timer]
OnCalendar=hourly
Persistent=true
RandomizedDelaySec=3min
[Install]
WantedBy=timers.target
EOF
systemctl --user daemon-reload
systemctl --user enable --now niruflow.timer
systemctl --user list-timers niruflow.timer
