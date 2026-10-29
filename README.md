# NIRUFLOW

**Local-first desktop automation and system maintenance for Linux.**

NIRUFLOW is a personal, local-first maintenance cockpit for file cleanup,
storage review, project artifacts, Docker and recurring rules. It keeps
destructive work explicit: scans are read-only, removals require confirmation,
and Nextcloud-synced paths get an extra warning because a move can
synchronize as a deletion on other devices.

## Current status

**0.5.0** — Experimental. Review every destructive action, especially cleanup
inside synchronized directories.

## Features

- File cleanup with protected names, quarantine and restore.
- Storage inventory, duplicate detection, large-file review and permanent
  deletion with content verification.
- Read-only project artifact and patch-chain inventory.
- Docker containers, images, Compose discovery and image-cleanup previews.
- Optional systemd user timer for selected automation rules.
- English UI with independent Swedish, UK or US regional date/time formatting.
- **Follow NIRUX desktop**: reads the active NIRUWM theme and applies its palette
  while NIRUFLOW is open. The ten built-in NIRUX themes are also available as
  fallbacks, and a custom file in `~/.config/niruwm/themes` takes precedence.

## Installation

Download the latest release and extract the archive:

```bash
cd NIRUFLOW-0.5.0
bash ./install.sh
~/.local/bin/niruflow
```

Requirements: Linux, Python 3.10+, `venv`, and `pip`. The desktop extra installs
PySide6. Existing configuration, history and quarantine are preserved by the
versioned installer.

## NIRUX integration

When used inside NIRUWM, choose **NIRUX desktop** in the theme selector. NIRUFLOW
then follows the current NIRUWM `theme` and its user theme file, including custom
themes, instead of maintaining a second stale palette.

NIRUX may offer NIRUFLOW as an optional suite extra. Extras are deliberately
outside the core NIRUX manifest: selecting one is explicit, and NIRUX core
updates do not silently install or replace it.

## Safety

- Never delete directories, symlinks or hard-linked files through storage tools.
- A changed inode, device, timestamp or content fingerprint blocks deletion.
- Nextcloud-synced paths require explicit confirmation.
- No cloud service, account or background daemon is required.

## License

[MIT](LICENSE) © 2026 **Ing Leif Nicklas Rudolfsson**.
