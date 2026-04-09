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

## Prepare To Test

If you only want to test the HTTP/API contract, the default mock backend is
enough.

If you want to test against a real Signal account, prepare:

- a dedicated Signal test account, not your primary personal account
- a second Signal account or device to send messages to and from the test
  account
- `protobuf-compiler` installed locally so the `presage` backend can build
- a local sqlite path for the `presage` store, for example
  `./signal-gatewayd.presage.sqlite`

Build the real backend:

```bash
cargo check -p gatewayd --features presage-backend
```

Run the daemon with the real backend selected:

```bash
SIGNAL_GATEWAY_BACKEND=presage \
SIGNAL_PRESAGE_DB_PATH=./signal-gatewayd.presage.sqlite \
SIGNAL_PRESAGE_DEVICE_NAME=signal-gatewayd \
cargo run -p gatewayd --features presage-backend
```

Then call:

```bash
curl -X POST http://127.0.0.1:3000/admin/link-device
```

and scan the returned provisioning URL with the primary Signal device to link
the gateway as a secondary device.

## How To Test

Current practical test flow:

1. Build and run the daemon.
2. For mock/backend API validation, run the normal Rust checks:

```bash
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

3. For live gateway smoke checks, export:

```bash
export SIGNAL_GATEWAY_ENABLE_MANUAL=1
export SIGNAL_GATEWAY_BASE_URL=http://127.0.0.1:3000
export SIGNAL_GATEWAY_ACCOUNT_ID=default
export SIGNAL_TEST_CONVERSATION_ID='<recipient or conversation id>'
```

4. Run:

```bash
./scripts/manual-real-signal-smoke.sh
```

Interpretation:

- on the mock backend, health/readiness and send/idempotency checks should pass
- on the current `presage` backend scaffold, health/link/load validation is the
  main thing that works today
- outbound send and attachment send are still not implemented on the real
  backend path, so full human-to-human messaging is not ready yet

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
