#!/usr/bin/env bash
#
# Extends the instance TTL and the uploaded wasm code TTL for every contract
# in deployments/<network>.env, so idle deployments (no calls for more than
# 7 days) do not archive on testnet.
#
# Usage:  ./scripts/extend-ttl.sh [network]
#         (network defaults to testnet)
#
# Environment:
#   EXTEND_IDENTITY  stellar identity used to sign the extend transactions
#                    (default: sci-admin).  On CI, populate the identity from
#                    EXTEND_SECRET before calling this script.
#   LEDGERS          how many ledgers to extend to (default: 518400 ≈ 30 days)
#
# CI usage (GitHub Actions):
#   - Store the funded key in a secret named EXTEND_SECRET.
#   - The workflow injects it as an env var and this script imports it.
#   - If EXTEND_SECRET is absent (forks, dependabot) the script exits 0 with a
#     notice rather than failing.
#
# What breaks if this is wrong: instance and code entries archive after the
# threshold lapses, and every subsequent call fails with an "entry not found"
# error until `stellar contract restore` is run.

set -euo pipefail

NETWORK="${1:-testnet}"
EXTEND_IDENTITY="${EXTEND_IDENTITY:-sci-admin}"
# 30 days in ledgers (matches INSTANCE_TTL_EXTEND in contracts/common/src/lib.rs)
LEDGERS="${LEDGERS:-518400}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="$ROOT/deployments/$NETWORK.env"

say() { printf '\n\033[1m==> %s\033[0m\n' "$1"; }
die() { printf 'error: %s\n' "$1" >&2; exit 1; }

command -v stellar >/dev/null || die "the stellar CLI is required: https://github.com/stellar/stellar-cli"

# On CI: if the secret is absent (fork or dependabot PR), skip gracefully so
# the workflow passes without exposing the key.
if [ -n "${EXTEND_SECRET:-}" ]; then
  stellar keys add "$EXTEND_IDENTITY" --secret-key --overwrite <<< "$EXTEND_SECRET" >/dev/null 2>&1 || \
    stellar keys import "$EXTEND_IDENTITY" "$EXTEND_SECRET" >/dev/null 2>&1 || true
elif ! stellar keys address "$EXTEND_IDENTITY" >/dev/null 2>&1; then
  printf 'notice: EXTEND_SECRET is not set and no %s identity exists locally.\n' "$EXTEND_IDENTITY"
  printf 'Skipping TTL extension (fork or dependabot without the secret).\n'
  exit 0
fi

[ -f "$ENV_FILE" ] || die "no deployment at $ENV_FILE; run scripts/deploy.sh $NETWORK first"
# shellcheck disable=SC1090
set -a; source "$ENV_FILE"; set +a

# contract-variable -> display name
CONTRACTS=(
  "REGISTRY_CONTRACT_ID:registry"
  "VOUCHER_CONTRACT_ID:voucher"
  "RECEIPT_CONTRACT_ID:receipt"
  "FAUCET_CONTRACT_ID:faucet"
)

remaining_ttl() {
  local id="$1"
  stellar contract info --id "$id" --network "$NETWORK" 2>/dev/null \
    | grep -i 'instance.*ttl\|ttl.*instance\|live_until_ledger' \
    | head -1 || true
}

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

for entry in "${CONTRACTS[@]}"; do
  var="${entry%%:*}"
  name="${entry##*:}"
  id="${!var:-}"
  [ -n "$id" ] || continue

  say "Extending $name ($id)"

  # Extend the contract instance (config, counters, admin).
  stellar contract extend \
    --id "$id" \
    --ledgers-to-expire "$LEDGERS" \
    --source-account "$EXTEND_IDENTITY" \
    --network "$NETWORK" \
    || { printf 'warning: could not extend instance for %s\n' "$name" >&2; continue; }
  printf '  instance extended to %s ledgers\n' "$LEDGERS"

  # Extend the uploaded wasm code entry so the bytecode does not archive.
  # Fetch the deployed wasm, compute its hash, then extend by hash.
  fetched="$TMP/${name}.wasm"
  if stellar contract fetch --id "$id" --network "$NETWORK" --out-file "$fetched" >/dev/null 2>&1; then
    wasm_hash="$(sha256sum "$fetched" | cut -d' ' -f1)"
    stellar contract extend \
      --wasm-hash "$wasm_hash" \
      --ledgers-to-expire "$LEDGERS" \
      --source-account "$EXTEND_IDENTITY" \
      --network "$NETWORK" \
      && printf '  code extended (hash %s…)\n' "${wasm_hash:0:12}" \
      || printf 'warning: could not extend code for %s\n' "$name" >&2
  else
    printf 'warning: could not fetch wasm for %s; skipping code extension\n' "$name" >&2
  fi
done

say "Done — contracts extended to $LEDGERS ledgers on $NETWORK"
