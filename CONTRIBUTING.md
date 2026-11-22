# Contributing

Use focused pull requests with a reproducible description and regression tests. Keep the UI neutral, keyboard-accessible and English-first with Swedish regional formatting. Never introduce unattended destructive actions, Docker volume removal or automatic Nextcloud deletion. Keep the command line JSON byte-compatible with earlier releases.

Before submitting:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
sh tests/xvfb.sh target/release/niruflow
sh tests/scripts.sh target/release/niruflow
```
