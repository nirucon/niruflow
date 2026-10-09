# NIRUFLOW

Local-first Linux maintenance and automation. Developed by **Ing Leif Nicklas Rudolfsson**. MIT licensed.

**Status:** `0.1.0-alpha.1` — development release. Review every destructive action before confirming.

## Features

- PySide6 desktop interface with Niru Noir and neutral themes.
- File cleanup with preview, quarantine, restore and history.
- Storage Explorer with filtering, sorting and duplicate inspection.
- Docker containers, images, Compose discovery and conservative cleanup previews.
- Read-only project inventory and media library overview.
- Optional systemd user scheduler, CLI diagnostics and configuration backup.

## Install

Python 3.10+ with `venv` and `pip` is required. On Debian/Ubuntu install `python3-venv` if missing.

```bash
unzip NIRUFLOW-0.1.0-alpha.1.zip
cd NIRUFLOW-0.1.0-alpha.1
bash ./install.sh
~/.local/bin/niruflow
```

The installer retains previous versions and never deletes configuration, history, quarantine or Docker volumes. Optional background scheduler: `bash ./install-scheduler.sh`.

## CLI

`niruflow status`, `niruflow doctor`, `niruflow rules`, `niruflow projects`, `niruflow media`, `niruflow scheduler-status`.

## Safety and limitations

Nextcloud-synced deletions synchronize to the server and other devices; deletions propagate to the server and other devices. Use preview and backups before deleting anything. Docker image sizes may include shared layers. File age is based on modification time. Background scheduling is hourly; full GUI and Docker integration testing is still needed before production use.

See [SECURITY.md](SECURITY.md) and [CONTRIBUTING.md](CONTRIBUTING.md).
