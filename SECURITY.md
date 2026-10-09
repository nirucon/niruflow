# Security policy

NIRUFLOW runs locally with the current user permissions. Never run as root.

Permanent deletion is irreversible locally; Nextcloud propagates deletions to other devices. Docker cleanup removes image references but never volumes automatically. Inspect every preview.

Report security issues privately to the maintainer instead of opening a public issue containing sensitive details. Never include real home paths, tokens, or private file names in public bug reports.

NIRUFLOW 0.1.0-alpha.1 does not guarantee race-free deletion against malicious concurrent filesystem mutation; avoid operating on untrusted writable directories.
