#!/usr/bin/env bash
#
# Deploys the testnet USDC faucet and hands it administration of the demo
# token, so anyone trying the demo can get test USDC from the app.
#
# Usage:  ./scripts/deploy-faucet.sh [network]     (default: testnet)
#
# Run after deploy.sh and seed.sh: once the faucet is the token admin, the
# issuer can no longer mint directly. To undo, call release_token_admin.
#
# Requires: stellar CLI >= 27, wasm-opt (binaryen 133), a funded source
# identity named $ADMIN.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/lib.sh
source "$ROOT/scripts/lib.sh"

usage() {
  cat <<'EOF'
Usage: ./scripts/deploy-faucet.sh [-h|--help] [--network <net>] [--dry-run] [network]

Deploys the testnet USDC faucet and hands it token administration.
Refuses to run on mainnet/public because the faucet mints to anyone.

Options:
  -h, --help          Show this message and exit.
  --network <net>     Network name (default: testnet). Positional form also
                      accepted for backwards compatibility.
  --dry-run           Print every command without sending a transaction or
                      uploading/deploying a contract.
EOF
}

NETWORK=""
DRY_RUN=0
while [ $# -gt 0 ]; do
  case "$1" in
    -h|--help) usage; exit 0 ;;
    --network) NETWORK="${2:?--network requires an argument}"; shift ;;
    --dry-run) DRY_RUN=1 ;;
    -*) die "unknown option: $1" ;;
    *)
      if [ -z "$NETWORK" ]; then NETWORK="$1"; else die "unexpected argument: $1"; fi
      shift; continue ;;
  esac
  shift
done
NETWORK="${NETWORK:-testnet}"

if [ "$NETWORK" = "mainnet" ] || [ "$NETWORK" = "public" ]; then
  die "The faucet mints to anyone. Refusing to deploy it on $NETWORK."
fi

ADMIN="${ADMIN:-sci-admin}"
ISSUER="${ISSUER:-sci-issuer}"
DRIP="${DRIP:-500000000}"          # 50 USDC at 7 decimals
COOLDOWN="${COOLDOWN:-86400}"      # once a day per address
DAILY_CAP="${DAILY_CAP:-2500000000}" # 250 USDC per day across all addresses
LIFETIME_CAP="${LIFETIME_CAP:-0}"   # 0 = unlimited; otherwise per address, ever

ENV_FILE="$ROOT/deployments/$NETWORK.env"
[ -f "$ENV_FILE" ] || die "No deployment found at $ENV_FILE. Run deploy.sh first."
# shellcheck disable=SC1090
set -a; source "$ENV_FILE"; set +a

ADMIN_ADDR="$(stellar keys address "$ADMIN")"

run() {
  if [ "$DRY_RUN" -eq 1 ]; then
    printf '  [dry-run] %s\n' "$*"
  else
    "$@"
  fi
}

say "Building contracts"
# The whole workspace is built, not just the faucet: cargo unifies features
# per package, so building one contract on its own produces different bytes.
# scripts/build-wasm.sh is the same build the release workflow performs.
if [ "$DRY_RUN" -eq 1 ]; then
  printf '  [dry-run] build-wasm.sh %s/dist\n' "$ROOT"
else
  "$ROOT/scripts/build-wasm.sh" "$ROOT/dist" >/dev/null
  grep sci_faucet "$ROOT/dist/SHA256SUMS"
fi

say "Deploying faucet"
if [ "$DRY_RUN" -eq 1 ]; then
  printf '  [dry-run] stellar contract deploy --wasm %s/dist/sci_faucet.wasm --source-account %s --network %s\n' \
    "$ROOT" "$ADMIN" "$NETWORK"
  FAUCET_ID="<dry-run-id>"
else
  FAUCET_ID="$(stellar contract deploy \
    --wasm "$ROOT/dist/sci_faucet.wasm" \
    --source-account "$ADMIN" --network "$NETWORK")"
fi
echo "  faucet: $FAUCET_ID"

say "Initializing faucet"
run stellar contract invoke --id "$FAUCET_ID" --source-account "$ADMIN" --network "$NETWORK" \
  -- initialize --admin "$ADMIN_ADDR" --token "$USDC_CONTRACT_ID" \
  --amount "$DRIP" --cooldown "$COOLDOWN" --daily-cap "$DAILY_CAP" \
  --lifetime-cap "$LIFETIME_CAP"

say "Handing token administration to the faucet"
run stellar contract invoke --id "$USDC_CONTRACT_ID" --source-account "$ISSUER" --network "$NETWORK" \
  -- set_admin --new_admin "$FAUCET_ID"

if [ "$DRY_RUN" -eq 0 ]; then
  grep -v '^FAUCET_CONTRACT_ID=' "$ENV_FILE" > "$ENV_FILE.tmp"
  echo "FAUCET_CONTRACT_ID=$FAUCET_ID" >> "$ENV_FILE.tmp"
  mv "$ENV_FILE.tmp" "$ENV_FILE"
fi

say "Done. Add to the frontend build environment:"
echo "  NEXT_PUBLIC_FAUCET_CONTRACT_ID=$FAUCET_ID"
