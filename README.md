# signal-gatewayd

`signal-gatewayd` is a local, Hermes-compatible Signal transport daemon.

Current status:

- Rust workspace with stable core types
- Mock Signal backend behind a trait boundary
- SQLite-backed local state bootstrap
- HTTP API for health, readiness, SSE events, JSON-RPC, and admin status

The current implementation is intentionally narrow:

- single account per process
- localhost bind by default
- bounded in-memory fanout via `tokio::broadcast`
- no full user CLI
- no real Signal network adapter yet

## CI

GitHub Actions runs:

- formatting checks
- clippy with warnings denied
- test suite
- a scheduled weekly `cargo update` compatibility run to surface upstream dependency breakage early

Dependabot is configured for Rust crates and GitHub Actions updates.

## Run

```bash
cargo run -p gatewayd
```

The daemon listens on `127.0.0.1:3000` by default and stores state in `./signal-gatewayd.db`.
