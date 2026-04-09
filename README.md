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

## Signal Backend Status

The repository does not yet include a `presage`-backed Signal adapter.

Current backend coverage is:

- mock backend contract tests in CI
- HTTP/API regression tests in CI
- manual live-gateway smoke tests for a running daemon

Once the real Signal backend lands, the manual smoke tests become the first line
of defense against upstream Signal-side protocol changes.

## CI

GitHub Actions runs:

- formatting checks
- clippy with warnings denied
- test suite
- a scheduled weekly `cargo update` compatibility run to surface upstream dependency breakage early

Dependabot is configured for Rust crates and GitHub Actions updates.

## Manual Smoke Tests

There is a manual live-gateway smoke test you can run against a locally running
daemon:

```bash
cargo test -p gatewayd --test manual_gateway_smoke -- --ignored --nocapture
```

See [docs/manual-real-signal-testing.md](/home/cctry/signal-gatewayd/docs/manual-real-signal-testing.md)
for the environment variables and recommended workflow.

## Run

```bash
cargo run -p gatewayd
```

The daemon listens on `127.0.0.1:3000` by default and stores state in `./signal-gatewayd.db`.
