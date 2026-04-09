#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ "${SIGNAL_GATEWAY_ENABLE_MANUAL:-0}" != "1" ]]; then
  cat <<'EOF'
SIGNAL_GATEWAY_ENABLE_MANUAL is not set to 1.

This script runs the ignored manual smoke tests against a live gateway.

Typical usage:
  export SIGNAL_GATEWAY_ENABLE_MANUAL=1
  export SIGNAL_GATEWAY_BASE_URL=http://127.0.0.1:3000
  export SIGNAL_GATEWAY_ACCOUNT_ID=default
  export SIGNAL_TEST_CONVERSATION_ID='<recipient or conversation id>'
  ./scripts/manual-real-signal-smoke.sh
EOF
  exit 1
fi

echo "Running manual gateway smoke tests"
echo "  base url: ${SIGNAL_GATEWAY_BASE_URL:-http://127.0.0.1:3000}"
echo "  account:  ${SIGNAL_GATEWAY_ACCOUNT_ID:-default}"
if [[ -n "${SIGNAL_TEST_CONVERSATION_ID:-}" ]]; then
  echo "  target:   ${SIGNAL_TEST_CONVERSATION_ID}"
else
  echo "  target:   <unset; send-path smoke test will self-skip>"
fi

cd "${ROOT_DIR}"
cargo test -p gatewayd --test manual_gateway_smoke -- --ignored --nocapture
