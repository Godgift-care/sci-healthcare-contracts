#!/usr/bin/env bash
#
# Restores a working SCI Healthcare deployment after a Stellar testnet reset.
#
# Usage:  ./scripts/after-reset.sh [--network testnet] [--rpc-url URL]
#
#   --network NET     network name as registered with the stellar CLI
#                     (default: testnet)
#   --rpc-url URL     override the RPC endpoint (optional; the CLI's registered
#                     testnet URL is used by default)
#   --dry-run         print every command without sending transactions or
#                     updating deployments/
#
# What this does (in order):
#   1. Funds the sci-admin and sci-issuer identities via friendbot.
#   2. Builds, deploys and initialises every contract (scripts/deploy.sh).
#   3. Seeds the demo clinic, attester and voucher (scripts/seed.sh).
#   4. Verifies wiring and bytecode (scripts/verify-deployment.sh,
#      scripts/verify-deployments.sh).
#   5. Optionally deploys the testnet faucet.
#   6. Prints the backend and frontend variables that must change.
#
# After this script succeeds, deployments/testnet.env and
# deployments/testnet.json hold the new addresses and wasm hashes.
# The backend (sci-healthcare-backend) and frontend (sci-healthcare-frontend)
# must be re-pointed at the new contract ids before the demo works again.
#
# What breaks if this is wrong: the demo stays down. Each step is guarded so a
# partial run can be resumed by re-running the script.

set -euo pipefail

NETWORK="testnet"
RPC_URL=""
DRY_RUN=0

usage() { sed -n '2,30p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; }

while [ $# -gt 0 ]; do
  case "$1" in
    --network) NETWORK="$2"; shift 2 ;;
    --network=*) NETWORK="${1#--network=}"; shift ;;
    --rpc-url) RPC_URL="$2"; shift 2 ;;
    --rpc-url=*) RPC_URL="${1#--rpc-url=}"; shift ;;
    --dry-run) DRY_RUN=1; shift ;;
    -h|--help) usage; exit 0 ;;
    -*) printf 'error: unknown option %s\n' "$1" >&2; exit 2 ;;
    *) printf 'error: unexpected argument %s\n' "$1" >&2; exit 2 ;;
  esac
done

ADMIN="${ADMIN:-sci-admin}"
ISSUER="${ISSUER:-sci-issuer}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="$ROOT/deployments/$NETWORK.env"

say()  { printf '\n\033[1m==> %s\033[0m\n' "$1"; }
die()  { printf 'error: %s\n' "$1" >&2; exit 1; }

run() {
  printf '+ %s\n' "$*"
  if [ "$DRY_RUN" -eq 0 ]; then "$@"; fi
}

command -v stellar >/dev/null || die "the stellar CLI is required"

ADMIN_ADDR="$(stellar keys address "$ADMIN" 2>/dev/null)" || \
  die "no identity named $ADMIN — run: stellar keys generate $ADMIN"
ISSUER_ADDR="$(stellar keys address "$ISSUER" 2>/dev/null)" || \
  die "no identity named $ISSUER — run: stellar keys generate $ISSUER"

RPC_FLAG=""
if [ -n "$RPC_URL" ]; then RPC_FLAG="--rpc-url $RPC_URL"; fi

# ── Step 1: Fund identities ────────────────────────────────────────────────

say "Funding admin and issuer via friendbot"
# shellcheck disable=SC2086
run stellar keys fund "$ADMIN" --network "$NETWORK" $RPC_FLAG  || \
  printf '  (admin already funded or friendbot unavailable)\n'
# shellcheck disable=SC2086
run stellar keys fund "$ISSUER" --network "$NETWORK" $RPC_FLAG || \
  printf '  (issuer already funded or friendbot unavailable)\n'
# Fund the seed identities so seed.sh does not need manual intervention.
for id in sci-clinic sci-attester sci-funder; do
  # shellcheck disable=SC2086
  run stellar keys fund "$id" --network "$NETWORK" $RPC_FLAG 2>/dev/null || \
    printf '  (%s already funded or key missing)\n' "$id"
done

# ── Step 2: Deploy ────────────────────────────────────────────────────────

say "Deploying contracts (scripts/deploy.sh $NETWORK --force)"
DEPLOY_ARGS=("$NETWORK" "--force")
[ "$DRY_RUN" -eq 1 ] && DEPLOY_ARGS+=("--dry-run")
run "$ROOT/scripts/deploy.sh" "${DEPLOY_ARGS[@]}"

# ── Step 3: Seed ──────────────────────────────────────────────────────────

if [ "$DRY_RUN" -eq 0 ]; then
  say "Seeding demo data (scripts/seed.sh $NETWORK)"
  run "$ROOT/scripts/seed.sh" "$NETWORK"
fi

# ── Step 4: Verify ────────────────────────────────────────────────────────

if [ "$DRY_RUN" -eq 0 ] && [ -f "$ENV_FILE" ]; then
  say "Verifying wiring"
  run "$ROOT/scripts/verify-deployment.sh" "$NETWORK" || \
    printf 'warning: wiring check failed — review the output above\n' >&2

  say "Verifying bytecode"
  run "$ROOT/scripts/verify-deployments.sh" "$NETWORK" --skip-build || \
    printf 'warning: bytecode check failed — review the output above\n' >&2
fi

# ── Step 5: Faucet ────────────────────────────────────────────────────────

if [ "$DRY_RUN" -eq 0 ] && [ "$NETWORK" = "testnet" ]; then
  say "Deploying faucet (testnet only)"
  run "$ROOT/scripts/deploy-faucet.sh" "$NETWORK" || \
    printf 'warning: faucet deploy failed — the faucet is optional\n' >&2
fi

# ── Step 6: Print follow-up variables ────────────────────────────────────

if [ "$DRY_RUN" -eq 0 ] && [ -f "$ENV_FILE" ]; then
  set -a; source "$ENV_FILE"; set +a
  say "Deployment complete — update these variables downstream"
  cat <<BLOCK

sci-healthcare-backend (.env or deployment config):
  REGISTRY_CONTRACT_ID=${REGISTRY_CONTRACT_ID:-<set after deploy>}
  VOUCHER_CONTRACT_ID=${VOUCHER_CONTRACT_ID:-<set after deploy>}
  RECEIPT_CONTRACT_ID=${RECEIPT_CONTRACT_ID:-<set after deploy>}
  USDC_CONTRACT_ID=${USDC_CONTRACT_ID:-<set after deploy>}

sci-healthcare-frontend (.env or deployment config):
  NEXT_PUBLIC_REGISTRY_CONTRACT_ID=${REGISTRY_CONTRACT_ID:-<set after deploy>}
  NEXT_PUBLIC_VOUCHER_CONTRACT_ID=${VOUCHER_CONTRACT_ID:-<set after deploy>}
  NEXT_PUBLIC_RECEIPT_CONTRACT_ID=${RECEIPT_CONTRACT_ID:-<set after deploy>}
  NEXT_PUBLIC_USDC_CONTRACT_ID=${USDC_CONTRACT_ID:-<set after deploy>}

Full addresses saved to:  $ENV_FILE
Wasm hashes saved to:     ${ENV_FILE%.env}.json

After updating the backend and frontend, restart both services.
BLOCK
else
  say "Dry run complete — no transactions were sent and $ENV_FILE was not changed"
fi
