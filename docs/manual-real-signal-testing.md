# Manual Real-Signal Testing

This repository now ships a feature-gated `presage` Signal transport scaffold.
The manual smoke suite in this document validates a running gateway over HTTP.
Today the real backend path is mainly useful for account load/link and compile
compatibility work; the outbound send path still needs a dedicated local worker
before it is ready for real end-to-end messaging.

## Why this exists

There are three different kinds of breakage we care about:

1. Rust API breakage from upstream dependencies.
2. Internal contract breakage in the gateway's HTTP/SSE/JSON-RPC surface.
3. Signal-side protocol or behavior breakage against a real linked account.

CI currently covers:

- Rust formatting/lint/test
- API contract tests against the mock backend
- weekly dependency update builds

CI does **not** currently prove that Signal protocol behavior still works against
the real network. It now does compile the real backend feature, which catches a
meaningful class of upstream breakage, but live-network validation still
requires a real account and should stay opt-in.

## Build the real backend

```bash
cargo check -p gatewayd --features presage-backend
```

Run the daemon with:

```bash
SIGNAL_GATEWAY_BACKEND=presage \
SIGNAL_PRESAGE_DB_PATH=./signal-gatewayd.presage.sqlite \
SIGNAL_PRESAGE_DEVICE_NAME=signal-gatewayd \
cargo run -p gatewayd --features presage-backend
```

## Required environment

Start a local `signal-gatewayd` that is already linked to your own account and
listening on localhost. Then export:

```bash
export SIGNAL_GATEWAY_ENABLE_MANUAL=1
export SIGNAL_GATEWAY_BASE_URL=http://127.0.0.1:3000
export SIGNAL_GATEWAY_ACCOUNT_ID=default
```

Meaning:

- `SIGNAL_GATEWAY_ENABLE_MANUAL=1`: an explicit safety switch so ignored manual
  tests do not run by accident.
- `SIGNAL_GATEWAY_BASE_URL`: the base URL of the running local daemon under
  test, usually `http://127.0.0.1:3000`.
- `SIGNAL_GATEWAY_ACCOUNT_ID`: the account identifier expected by the gateway.
  Today that defaults to `default`.

For send-path smoke tests, also export a target conversation:

```bash
export SIGNAL_TEST_CONVERSATION_ID='<recipient or conversation id>'
```

Meaning:

- `SIGNAL_TEST_CONVERSATION_ID`: the conversation or recipient ID that the
  gateway should send to for the manual send-path smoke test. If you leave it
  unset, health/readiness checks still run, but the send smoke test self-skips.

## Run the smoke tests

```bash
./scripts/manual-real-signal-smoke.sh
```

## What the smoke tests validate

- `/health` returns 200
- `/ready` returns either `200` or `503` with a reachable daemon
- `send` over JSON-RPC succeeds when a target conversation is configured
- idempotency returns the same `message_id` for repeated requests

At the moment, the manual send-path smoke test is only expected to succeed on
the mock backend. The `presage` backend still returns an explicit unimplemented
error on outbound send operations.

## Recommended protocol-change checklist

Run this before and after bumping a real Signal backend dependency such as
`presage`:

1. `cargo test --workspace --all-targets`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. Start a locally linked gateway with the real backend enabled.
4. Run the ignored manual smoke suite.
5. Send a real message to a test contact or test group.
6. Verify receipt, reconnect, and attachment behavior manually.

## What to add once the real backend lands

When the `presage` adapter exists, extend the manual suite with:

- attachment upload/download smoke cases
- inbound SSE receive validation
- daemon restart and checkpoint replay validation
- linking flow validation on a disposable test account

At that point, we should also add a dedicated CI job that compiles the real
backend feature on every PR, even if live-network tests remain local only.
