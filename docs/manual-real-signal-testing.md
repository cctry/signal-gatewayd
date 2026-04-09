# Manual Real-Signal Testing

This repository does not yet ship a `presage`-backed Signal transport. The
manual smoke suite in this document is meant to validate a running gateway over
HTTP, and becomes the protocol smoke suite once the real Signal backend is
implemented.

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
the real network. That requires a real account and should stay opt-in.

## Required environment

Start a local `signal-gatewayd` that is already linked to your own account and
listening on localhost. Then export:

```bash
export SIGNAL_GATEWAY_ENABLE_MANUAL=1
export SIGNAL_GATEWAY_BASE_URL=http://127.0.0.1:3000
export SIGNAL_GATEWAY_ACCOUNT_ID=default
```

For send-path smoke tests, also export a target conversation:

```bash
export SIGNAL_TEST_CONVERSATION_ID='<recipient or conversation id>'
```

If your send flow requires a specific account identifier, set:

```bash
export SIGNAL_GATEWAY_ACCOUNT_ID='<account id>'
```

## Run the smoke tests

```bash
cargo test -p gatewayd --test manual_gateway_smoke -- --ignored --nocapture
```

## What the smoke tests validate

- `/health` returns 200
- `/ready` returns either `200` or `503` with a reachable daemon
- `send` over JSON-RPC succeeds when a target conversation is configured
- idempotency returns the same `message_id` for repeated requests

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
