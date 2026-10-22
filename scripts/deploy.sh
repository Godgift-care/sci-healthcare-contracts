#!/usr/bin/env bash
#
# Deploys the SCI Healthcare contracts in dependency order and wires them
# together. Prints a copy-pasteable environment block at the end.
#
# Usage:  ./scripts/deploy.sh [network] [--force] [--dry-run]
#         (network defaults to testnet)
#
#   --force    replace deployments/<network>.env if it already exists
#   --dry-run  print every command without building, deploying or sending
#              a transaction, and leave deployments/ untouched
#
# Requires: stellar CLI >= 27, wasm-opt (binaryen 133), funded source
# identities named $ADMIN and $ISSUER.
#
# The contracts are built by scripts/build-wasm.sh, the same script the
# release workflow uses, so what is deployed is what is released and
# scripts/verify-deployments.sh can check it.
#
# What breaks if this is wrong: running it twice replaces the live
# addresses in deployments/<network>.env, and the app and indexer point at
# contracts nobody is using. Existing deployments are therefore refused
# unless --force is explicit.

set -euo pipefail

NETWORK=""
FORCE=0
DRY_RUN=0

usage() {
  sed -n '2,20p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

while [ $# -gt 0 ]; do
  case "$1" in
    --force) FORCE=1; shift ;;
    --dry-run) DRY_RUN=1; shift ;;
    -h|--help) usage; exit 0 ;;
    -*) printf 'error: unknown option %s\n' "$1" >&2; exit 2 ;;
    *)
      if [ -z "$NETWORK" ]; then
        NETWORK="$1"
      else
        printf 'error: unexpected argument %s\n' "$1" >&2
        exit 2
      fi
      shift
      ;;
  esac
done
NETWORK="${NETWORK:-testnet}"

ADMIN="${ADMIN:-sci-admin}"
ISSUER="${ISSUER:-sci-issuer}"
DISPUTE_WINDOW="${DISPUTE_WINDOW:-259200}"   # 72 hours, in seconds
FEE_BPS="${FEE_BPS:-100}"                    # 1%
ATTESTER_SHARE_BPS="${ATTESTER_SHARE_BPS:-50}" # half the fee, in bps of the amount

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WASM_DIR="$ROOT/dist"
OUT="$ROOT/deployments/$NETWORK.env"

say() { printf '\n\033[1m==> %s\033[0m\n' "$1"; }
die() { printf 'error: %s\n' "$1" >&2; exit 1; }

quote_cmd() {
  local out="" q=""
  for arg in "$@"; do
    printf -v q '%q' "$arg"
    out="${out:+$out }$q"
  done
  printf '%s' "$out"
}

echo_cmd() { printf '+ %s\n' "$(quote_cmd "$@")"; }

run() {
  echo_cmd "$@"
  if [ "$DRY_RUN" -eq 0 ]; then
    "$@"
  fi
}

run_quiet_or_already_deployed() {
  printf '+ %s >/dev/null 2>&1 || echo "  (already deployed)"\n' "$(quote_cmd "$@")"
  if [ "$DRY_RUN" -eq 0 ]; then
    "$@" >/dev/null 2>&1 || echo "  (already deployed)"
  fi
}

# Runs a command and stores its stdout in the named variable. In a dry run
# the command is printed and a visible placeholder is stored instead, so
# later commands can be rendered without contacting the network.
capture() {
  local var="$1"
  shift
  printf '+ %s="$(%s)"\n' "$var" "$(quote_cmd "$@")"
  if [ "$DRY_RUN" -eq 1 ]; then
    printf -v "$var" '<%s>' "$var"
    return 0
  fi
  local value
  value="$("$@")"
  printf -v "$var" '%s' "$value"
}

if [ -f "$OUT" ]; then
  if [ "$DRY_RUN" -eq 1 ]; then
    say "Existing deployment found"
    printf '  %s already exists; --dry-run will not touch it.\n' "$OUT"
  elif [ "$FORCE" -ne 1 ]; then
    die "refusing to overwrite $OUT. Re-run with --force only when you intend to replace the live deployment, or use --dry-run to preview the commands."
  fi
fi

run mkdir -p "$(dirname "$OUT")"

capture ADMIN_ADDR stellar keys address "$ADMIN"
capture ISSUER_ADDR stellar keys address "$ISSUER"

say "Building contracts"
if [ "$DRY_RUN" -eq 1 ]; then
  printf '+ %s >/dev/null\n' "$(quote_cmd "$ROOT/scripts/build-wasm.sh" "$WASM_DIR")"
  printf '+ cat %q\n' "$WASM_DIR/SHA256SUMS"
else
  "$ROOT/scripts/build-wasm.sh" "$WASM_DIR" >/dev/null
  cat "$WASM_DIR/SHA256SUMS"
fi

say "Deploying settlement token (USDC test asset)"
run_quiet_or_already_deployed stellar contract asset deploy \
  --asset "USDC:$ISSUER_ADDR" \
  --source-account "$ISSUER" \
  --network "$NETWORK"
capture USDC_ID stellar contract id asset \
  --asset "USDC:$ISSUER_ADDR" \
  --network "$NETWORK"
echo "  USDC: $USDC_ID"

# Contracts deploy in dependency order: registry and receipt own no
# references, voucher points at both.
say "Deploying registry"
capture REGISTRY_ID stellar contract deploy \
  --wasm "$WASM_DIR/sci_registry.wasm" \
  --source-account "$ADMIN" --network "$NETWORK"
echo "  registry: $REGISTRY_ID"

say "Deploying receipt book"
capture RECEIPT_ID stellar contract deploy \
  --wasm "$WASM_DIR/sci_receipt.wasm" \
  --source-account "$ADMIN" --network "$NETWORK"
echo "  receipt:  $RECEIPT_ID"

say "Deploying voucher escrow"
capture VOUCHER_ID stellar contract deploy \
  --wasm "$WASM_DIR/sci_voucher.wasm" \
  --source-account "$ADMIN" --network "$NETWORK"
echo "  voucher:  $VOUCHER_ID"

say "Initializing registry"
run stellar contract invoke --id "$REGISTRY_ID" --source-account "$ADMIN" \
  --network "$NETWORK" -- initialize --admin "$ADMIN_ADDR"

# The receipt book is initialised after both contracts exist, so it can name
# the voucher contract as minter at once. Changing the minter later is a
# two-step handover (`propose_minter` / `accept_minter`).
say "Initializing receipt book"
run stellar contract invoke --id "$RECEIPT_ID" --source-account "$ADMIN" \
  --network "$NETWORK" -- initialize --admin "$ADMIN_ADDR" --minter "$VOUCHER_ID"

say "Initializing voucher escrow"
run stellar contract invoke --id "$VOUCHER_ID" --source-account "$ADMIN" \
  --network "$NETWORK" -- initialize \
  --admin "$ADMIN_ADDR" \
  --registry "$REGISTRY_ID" \
  --receipt_book "$RECEIPT_ID" \
  --token "$USDC_ID" \
  --dispute_window "$DISPUTE_WINDOW" \
  --fee_bps "$FEE_BPS" \
  --attester_share_bps "$ATTESTER_SHARE_BPS" \
  --fee_account "$ADMIN_ADDR"

if [ "$DRY_RUN" -eq 1 ]; then
  printf '+ cat > %q <<ENVEOF\n' "$OUT"
  printf '  STELLAR_NETWORK=%s\n' "$NETWORK"
  printf '  REGISTRY_CONTRACT_ID=%s\n' "$REGISTRY_ID"
  printf '  VOUCHER_CONTRACT_ID=%s\n' "$VOUCHER_ID"
  printf '  RECEIPT_CONTRACT_ID=%s\n' "$RECEIPT_ID"
  printf '  USDC_CONTRACT_ID=%s\n' "$USDC_ID"
  printf '  USDC_ISSUER=%s\n' "$ISSUER_ADDR"
  printf '  ADMIN_ADDRESS=%s\n' "$ADMIN_ADDR"
  printf 'ENVEOF\n'

  say "Dry run complete — no transactions were sent"
  cat <<BLOCK

Nothing was built, deployed, signed or written to $OUT.

  network:  $NETWORK
  admin:    $ADMIN_ADDR
  issuer:   $ISSUER_ADDR
  registry: $REGISTRY_ID
  voucher:  $VOUCHER_ID
  receipt:  $RECEIPT_ID
  USDC:     $USDC_ID

Re-run without --dry-run to deploy. If $OUT already exists, add --force
only when you intend to replace it.
BLOCK
  exit 0
fi

cat > "$OUT" <<ENVEOF
STELLAR_NETWORK=$NETWORK
REGISTRY_CONTRACT_ID=$REGISTRY_ID
VOUCHER_CONTRACT_ID=$VOUCHER_ID
RECEIPT_CONTRACT_ID=$RECEIPT_ID
USDC_CONTRACT_ID=$USDC_ID
USDC_ISSUER=$ISSUER_ADDR
ADMIN_ADDRESS=$ADMIN_ADDR
ENVEOF

say "Deployed — copy into your backend and frontend .env files"
cat <<BLOCK

  network:  $NETWORK
  admin:    $ADMIN_ADDR
  registry: $REGISTRY_ID
  voucher:  $VOUCHER_ID
  receipt:  $RECEIPT_ID
  USDC:     $USDC_ID

Saved to $OUT

Next steps:
  1. ./scripts/seed.sh $NETWORK
  2. ./scripts/verify-deployments.sh $NETWORK
  3. On testnet only: ./scripts/deploy-faucet.sh $NETWORK
  4. Before mainnet: follow docs/developers/mainnet-checklist.md
BLOCK
