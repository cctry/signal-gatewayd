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
The repository now includes a feature-gated Whisperfish `presage` integration
scaffold, but it is not yet full end-to-end parity.

Current backend coverage is:

- mock backend contract tests in CI
- HTTP/API regression tests in CI
- manual live-gateway smoke tests for a running daemon
- `cargo check -p gatewayd --features presage-backend` in CI to catch upstream
  compile-time breakage from the real Signal dependency stack

Once the real Signal backend lands, the manual smoke tests become the first line
of defense against upstream Signal-side protocol changes.

## CI

GitHub Actions runs:

- formatting checks
- clippy with warnings denied
- test suite
- a scheduled weekly `cargo update` compatibility run to surface upstream dependency breakage early

Dependabot is configured for Rust crates and GitHub Actions updates.

## Presage Backend

To build the real Signal backend scaffold:

```bash
cargo check -p gatewayd --features presage-backend
```

To run with that backend selected:

```bash
SIGNAL_GATEWAY_BACKEND=presage \
SIGNAL_PRESAGE_DB_PATH=./signal-gatewayd.presage.sqlite \
SIGNAL_PRESAGE_DEVICE_NAME=signal-gatewayd \
cargo run -p gatewayd --features presage-backend
```

Current state of the `presage` backend scaffold:

- real Whisperfish `presage` and `presage-store-sqlite` dependencies
- real account load/link scaffolding
- real inbound receive-loop scaffolding on a dedicated local Tokio runtime thread
- send/sendAttachment are still explicitly unimplemented in this backend path
  because upstream `presage` send futures are not `Send` and need a dedicated
  command worker architecture

## Manual Smoke Tests

There is a manual live-gateway smoke test you can run against a locally running
daemon:

```bash
export SIGNAL_GATEWAY_ENABLE_MANUAL=1
export SIGNAL_GATEWAY_BASE_URL=http://127.0.0.1:3000
export SIGNAL_GATEWAY_ACCOUNT_ID=default
export SIGNAL_TEST_CONVERSATION_ID='<recipient or conversation id>'
./scripts/manual-real-signal-smoke.sh
```

See [docs/manual-real-signal-testing.md](/home/cctry/signal-gatewayd/docs/manual-real-signal-testing.md)
for the environment variables and recommended workflow.

## Run

```bash
cargo run -p gatewayd
```

The daemon listens on `127.0.0.1:3000` by default and stores state in `./signal-gatewayd.db`.
