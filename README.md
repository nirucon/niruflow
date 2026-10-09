# NIRUFLOW

**Local-first desktop automation and system maintenance for Linux.**

NIRUFLOW is built by me, for me, to make my own everyday workflows simpler. The idea is to bring practical tools for file cleanup, storage, Docker and recurring maintenance tasks into one straightforward desktop application, with an emphasis on visibility, control and safer operations.

This is a personal experiment, not a finished product. It is still in early development, features may change, and things may break. I am sharing the source in case it is useful to someone else.

## Current status

**0.1.0-alpha.1** — Experimental alpha. Review operations carefully, especially deletion and cleanup. Changes to Nextcloud-synced files may propagate to other devices.

## Installation

Download the [latest release](https://github.com/nirucon/niruflow/releases) and extract the archive:

```bash
cd NIRUFLOW-0.1.0-alpha.1
bash ./install.sh
~/.local/bin/niruflow
```

Requires Linux and Python 3.10+ with `venv` and `pip`.

## License

[MIT](LICENSE) © 2026 **Ing Leif Nicklas Rudolfsson**.
