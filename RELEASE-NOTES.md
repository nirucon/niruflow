# NIRUFLOW 1.0.0

A rewrite in Rust with full feature parity: one static binary instead of a
Python virtual environment with PySide6.

- The window is drawn on its own canvas over X11 (x11rb, no Qt, GTK or web
  engine) in the NIRUFM style, with all nine pages, dialogs and typed
  confirmations of 0.x. It shows its first frame in about 11 to 40 ms and
  idles at about 13 MB (0.6.1: about 530 ms and 90 MB) with no CPU use.
- Scans run on worker threads with a parallel directory walk; the window
  never blocks and storage scans can be cancelled. `large-files` on a real
  home went from 128 s to 0.6 s.
- The command line is the same, with byte-compatible JSON. `large-files`
  lists the true largest files (no 20,000-file cut-off) once each when
  locations overlap; `forgotten` returns the oldest files; `projects`, which
  failed in 0.6.1, works again; `doctor` reports `python`/`pyside6` as not
  required.
- Deletion is as guarded as before: quarantine by atomic rename that never
  overwrites, restore from History, revalidated identity and content
  fingerprints, the sync-folder guards and the typed DELETE, SYNC DELETE,
  DELETE IMAGES and REMOVE CONTAINER confirmations.
- The NIRUX desktop theme follows NIRUWM live through the root window
  property and inotify instead of a 1.5 s timer.
- `config.json`, `history.sqlite3`, the quarantine and backup ZIPs from 0.x
  are used unchanged. `install.sh` downloads the release binary, verifies
  its SHA-256 (or builds with `--build`), replaces a 0.x install and points
  an installed scheduler unit at the new binary.

# NIRUFLOW 0.6.1

Installer and uninstaller release; the application is unchanged.

- New `uninstall.sh` (and `nirux extras uninstall niruflow`): disables and
  removes the scheduler timer, the launcher, desktop entry, icon and installed
  versions, and keeps rules, history and quarantine unless `--purge` is given.
- On a fresh Debian system the installer installs `python3-venv` when venv
  support is missing (asks in a terminal; `NIRUFLOW_ASSUME_YES=1` or
  `NIRUX_ASSUME_YES=1` unattended) instead of failing.
- A virtual environment left without pip by a failed attempt is rebuilt.

# NIRUFLOW 0.6.0

This release tightens the parts that could otherwise look finished while still
being unsafe.

- GUI storage rows now retain device, inode, ctime and content fingerprint
  data until deletion, so the protection is actually used by the interface.
- Full SHA-256 for files up to 64 MiB; larger files use a bounded first/last
  block fingerprint and the UI states that limitation.
- Sync protection covers Nextcloud, ownCloud, Dropbox, OneDrive, Google Drive,
  MEGAsync, pCloud, iCloud Drive, Seafile, Syncthing and rclone path roots.
- **NIRUX desktop** theme follows the active NIRUWM theme live while the window
  is open.
- PySide6 is bounded to the 6.x major series.
- Offscreen GUI smoke mode (`NIRUFLOW_SMOKE=1`) added for CI/release checks.
- Desktop icon added.
