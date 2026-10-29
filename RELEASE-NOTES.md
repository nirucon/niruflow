# NIRUFLOW 0.5.0

This release tightens deletion safety and makes NIRUFLOW part of the NIRUX
desktop instead of a lookalike.

- Storage deletion now verifies device, inode, ctime and a bounded content
  fingerprint before unlinking. Same-size rewrites with coarse filesystem
  timestamps are rejected.
- **NIRUX desktop** theme follows the active NIRUWM theme and user theme file.
  All ten built-in NIRUX palettes are available as fallbacks.
- Version and release metadata are synchronized across the CLI, GUI, installer
  and tests.
- The README reflects the current feature set, safety model and NIRUX integration.
