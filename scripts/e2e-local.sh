#!/usr/bin/env bash
#
# End-to-end check against a real Stellar node: deploy the contracts with
# scripts/deploy.sh, seed them with scripts/seed.sh, and assert that voucher 1
# really reached `Attested` on chain.
#
# Usage:  ./scripts/e2e-local.sh [rpc-url]
#
#   rpc-url            Soroban RPC endpoint (default http://localhost:8000/soroban/rpc)
#
# Environment:
#   NETWORK            network name to register with the CLI (default: local)
#   RPC_URL            overrides the positional rpc-url
#   NETWORK_PASSPHRASE passphrase of the node (default: the standalone network)
#   WAIT_SECS          how long to wait for the node (default: 120)
#
# Expects a node already running in local mode, for example:
#
#   docker run --rm -d -p 8000:8000 --name stellar stellar/quickstart:latest --local
#
# What breaks if this is wrong: `cargo test` runs in an in-process host, so it
# never exercises the CLI flags, the RPC or the deploy/seed scripts. A broken
# script is only found by a person deploying a release, on a network where the
# mistake is more expensive. This is the check that catches it first.

set -euo pipefail

NETWORK="${NETWORK:-local}"
RPC_URL="${RPC_URL:-${1:-http://localhost:8000/soroban/rpc}}"
NETWORK_PASSPHRASE="${NETWORK_PASSPHRASE:-Standalone Network ; February 2017}"
WAIT_SECS="${WAIT_SECS:-120}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="$ROOT/deployments/$NETWORK.env"
STEP="startup"

step() {
  STEP="$1"
  printf '\n\033[1m==> %s\033[0m\n' "$1"
}

on_error() {
  printf '\n\033[31mFAILED at: %s\033[0m\n' "$STEP" >&2
  printf 'Set DEBUG=1 and re-run with `bash -x` for the command that failed.\n' >&2
}

trap on_error ERR
set -E

require() {
  command -v "$1" >/dev/null || {
    printf 'error: %s is required but was not found on PATH\n' "$1" >&2
    exit 1
  }
}

rpc_health() {
  curl -sf -X POST "$RPC_URL" \
    -H 'Content-Type: application/json' \
    -d '{"jsonrpc":"2.0","id":1,"method":"getHealth"}' 2>/dev/null | grep -q '"status":"healthy"'
}

require curl
require jq
require stellar

step "Checking tools"
stellar --version | head -1
command -v wasm-opt >/dev/null && wasm-opt --version
printf 'network: %s\nrpc:     %s\n' "$NETWORK" "$RPC_URL"

step "Waiting for the node to be healthy (${WAIT_SECS}s)"
healthy=0
for _ in $(seq 1 "$WAIT_SECS"); do
  if rpc_health; then
    healthy=1
    break
  fi
  sleep 1
done
if [ "$healthy" -ne 1 ]; then
  printf 'error: no healthy Soroban RPC at %s after %ss.\n' "$RPC_URL" "$WAIT_SECS" >&2
  exit 1
fi
curl -sf -X POST "$RPC_URL" \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"getLatestLedger"}'
printf '\n'

step "Registering the network with the CLI"
stellar network rm "$NETWORK" >/dev/null 2>&1 || true
stellar network add "$NETWORK" \
  --rpc-url "$RPC_URL" \
  --network-passphrase "$NETWORK_PASSPHRASE"

step "Creating and funding identities"
# The same names deploy.sh and seed.sh expect, so the two scripts need no
# other configuration.
for id in sci-admin sci-issuer sci-clinic sci-attester sci-funder; do
  stellar keys generate "$id" --network "$NETWORK" --fund --overwrite >/dev/null
  printf '  %-14s %s\n' "$id" "$(stellar keys address "$id")"
done

step "Deploying with scripts/deploy.sh"
# --force when a previous local run left its env file behind: this is a
# throwaway node, and refusing would make the script unrunnable twice.
if [ -f "$ENV_FILE" ]; then
  "$ROOT/scripts/deploy.sh" "$NETWORK" --force
else
  "$ROOT/scripts/deploy.sh" "$NETWORK"
fi

step "Seeding with scripts/seed.sh"
"$ROOT/scripts/seed.sh" "$NETWORK"

step "Checking that voucher 1 is Attested on chain"
# shellcheck disable=SC1090
set -a
source "$ENV_FILE"
set +a

VOUCHER="$(stellar contract invoke \
  --id "$VOUCHER_CONTRACT_ID" \
  --source-account sci-admin \
  --network "$NETWORK" \
  -- get_voucher --voucher_id 1)"

printf '%s\n' "$VOUCHER"

# VoucherStatus is a unit enum with explicit discriminants, so the JSON
# carries the number: 2 is Attested. Accept either the number or the name so
# a CLI that renders enums differently does not fail the job for the wrong
# reason.
STATUS="$(printf '%s' "$VOUCHER" | jq -r '.status' 2>/dev/null || true)"
if [ "$STATUS" != "2" ] && [ "$STATUS" != "Attested" ]; then
  printf '\nerror: voucher 1 was seeded but is not Attested (status: %s).\n' "$STATUS" >&2
  printf 'A Funded voucher means seed.sh stopped between funding and attesting;\n' >&2
  printf 'a Claimed one means the attestation call failed. The output above names\n' >&2
  printf 'the contract state that was actually reached.\n' >&2
  exit 1
fi

# The provider was paid nothing yet — settlement waits for the dispute window
# — so also prove the receipt does not exist early.
RECEIPT="$(stellar contract invoke \
  --id "$RECEIPT_CONTRACT_ID" \
  --source-account sci-admin \
  --network "$NETWORK" \
  -- count_for --beneficiary_ref "$(printf 'patient-demo-001' | sha256sum | cut -d' ' -f1)" 2>/dev/null || echo 0)"
printf 'receipts for the demo beneficiary before settlement: %s\n' "$RECEIPT"

printf '\n\033[32mEnd-to-end deployment check passed: voucher 1 is Attested.\033[0m\n'
printf 'Addresses are in deployments/%s.env\n' "$NETWORK"
