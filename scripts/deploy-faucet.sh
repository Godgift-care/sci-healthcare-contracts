#!/usr/bin/env bash
#
# Deploys the testnet USDC faucet and hands it administration of the demo
# token, so anyone trying the demo can get test USDC from the app.
#
# Usage:  ./scripts/deploy-faucet.sh [network]     (default: testnet)
#
# Run after deploy.sh and seed.sh: once the faucet is the token admin, the
# issuer can no longer mint directly. To undo, call release_token_admin.

set -euo pipefail

NETWORK="${1:-testnet}"
if [ "$NETWORK" = "mainnet" ] || [ "$NETWORK" = "public" ]; then
  echo "The faucet mints to anyone. Refusing to deploy it on $NETWORK." >&2
  exit 1
fi

ADMIN="${ADMIN:-sci-admin}"
ISSUER="${ISSUER:-sci-issuer}"
DRIP="${DRIP:-500000000}"          # 50 USDC at 7 decimals
COOLDOWN="${COOLDOWN:-86400}"      # once a day per address

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="$ROOT/deployments/$NETWORK.env"
[ -f "$ENV_FILE" ] || { echo "No deployment found at $ENV_FILE. Run deploy.sh first."; exit 1; }
# shellcheck disable=SC1090
set -a; source "$ENV_FILE"; set +a

say() { printf '\n\033[1m==> %s\033[0m\n' "$1"; }
ADMIN_ADDR="$(stellar keys address "$ADMIN")"

say "Building faucet"
(cd "$ROOT" && cargo build --target wasm32v1-none --release -p sci-faucet)

say "Deploying faucet"
FAUCET_ID="$(stellar contract deploy \
  --wasm "$ROOT/target/wasm32v1-none/release/sci_faucet.wasm" \
  --source-account "$ADMIN" --network "$NETWORK")"
echo "  faucet: $FAUCET_ID"

say "Initializing faucet"
stellar contract invoke --id "$FAUCET_ID" --source-account "$ADMIN" --network "$NETWORK" \
  -- initialize --admin "$ADMIN_ADDR" --token "$USDC_CONTRACT_ID" \
  --amount "$DRIP" --cooldown "$COOLDOWN"

say "Handing token administration to the faucet"
stellar contract invoke --id "$USDC_CONTRACT_ID" --source-account "$ISSUER" --network "$NETWORK" \
  -- set_admin --new_admin "$FAUCET_ID"

grep -v '^FAUCET_CONTRACT_ID=' "$ENV_FILE" > "$ENV_FILE.tmp"
echo "FAUCET_CONTRACT_ID=$FAUCET_ID" >> "$ENV_FILE.tmp"
mv "$ENV_FILE.tmp" "$ENV_FILE"

say "Done. Add to the frontend build environment:"
echo "  NEXT_PUBLIC_FAUCET_CONTRACT_ID=$FAUCET_ID"
