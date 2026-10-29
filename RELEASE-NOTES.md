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
