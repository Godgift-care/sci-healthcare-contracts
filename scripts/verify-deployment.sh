#!/usr/bin/env bash
#
# Checks that a deployed set of contracts is wired together the way
# deployments/<network>.env says: the voucher points at the right registry,
# receipt book and token, the receipt book's minter is the voucher, every
# contract has the expected admin, and the fee is what was intended.
#
# Usage:  ./scripts/verify-deployment.sh [network] [--env-file FILE]
#
#   network          testnet (default), mainnet, local, ...
#   --env-file FILE  compare against FILE instead of deployments/<network>.env
#
# Environment (expected values; defaults are what scripts/deploy.sh uses):
#   FEE_BPS          expected protocol fee           (default 100)
#   DISPUTE_WINDOW   expected dispute window, s      (default 259200)
#   FEE_ACCOUNT      expected fee account            (default ADMIN_ADDRESS)
#   SOURCE           identity used to simulate reads (default sci-admin)
#
# Every call is a read-only simulation: nothing is signed or sent.
#
# This is the wiring check. scripts/verify-deployments.sh is the bytecode
# check — that the deployed wasm is what this repository builds. Run both.
#
# Each check prints PASS or FAIL; the script exits non-zero if any fails.
#
# What breaks if this is wrong: a deployment wired to the wrong registry, or
# whose receipt book will not accept the voucher as minter, looks healthy
# until the first settlement fails.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NETWORK=""
ENV_FILE=""
while [ $# -gt 0 ]; do
  case "$1" in
    --env-file)
      [ $# -ge 2 ] || { printf 'error: --env-file needs a path\n' >&2; exit 2; }
      ENV_FILE="$2"; shift 2 ;;
    --env-file=*) ENV_FILE="${1#--env-file=}"; shift ;;
    -h|--help) sed -n '2,29p' "${BASH_SOURCE[0]}"; exit 0 ;;
    -*) printf 'error: unknown option %s\n' "$1" >&2; exit 2 ;;
    *)
      if [ -z "$NETWORK" ]; then NETWORK="$1"; else printf 'error: unexpected argument %s\n' "$1" >&2; exit 2; fi
      shift ;;
  esac
done
NETWORK="${NETWORK:-testnet}"
ENV_FILE="${ENV_FILE:-$ROOT/deployments/$NETWORK.env}"

die() { printf 'error: %s\n' "$1" >&2; exit 1; }
command -v stellar >/dev/null || die "the stellar CLI is required: https://github.com/stellar/stellar-cli"
command -v jq >/dev/null || die "jq is required"
[ -f "$ENV_FILE" ] || die "no deployment file at $ENV_FILE"
# shellcheck disable=SC1090
set -a; source "$ENV_FILE"; set +a

for var in REGISTRY_CONTRACT_ID VOUCHER_CONTRACT_ID RECEIPT_CONTRACT_ID USDC_CONTRACT_ID ADMIN_ADDRESS; do
  [ -n "${!var:-}" ] || die "$var is not set in $ENV_FILE"
done

SOURCE="${SOURCE:-sci-admin}"
EXPECT_FEE_BPS="${FEE_BPS:-100}"
EXPECT_WINDOW="${DISPUTE_WINDOW:-259200}"
EXPECT_FEE_ACCOUNT="${FEE_ACCOUNT:-$ADMIN_ADDRESS}"

view() { # <contract-id> <function> -> raw JSON, or nothing if the call fails
  stellar contract invoke --id "$1" --source-account "$SOURCE" --network "$NETWORK" \
    --send=no -- "$2" 2>/dev/null || true
}

passes=0
failures=0
check() { # <description> <expected> <actual>
  if [ -n "$3" ] && [ "$2" = "$3" ]; then
    printf 'PASS  %s\n' "$1"
    passes=$((passes + 1))
  else
    printf 'FAIL  %s\n      expected %s\n      found    %s\n' "$1" "$2" "${3:-(no answer)}"
    failures=$((failures + 1))
  fi
}

printf 'Checking the %s deployment against %s\n\n' "$NETWORK" "$ENV_FILE"

config="$(view "$VOUCHER_CONTRACT_ID" get_config)"
field() { [ -n "$config" ] && printf '%s' "$config" | jq -r ".$1 // empty" 2>/dev/null || true; }
check "voucher.registry is the registry"         "$REGISTRY_CONTRACT_ID" "$(field registry)"
check "voucher.receipt_book is the receipt book" "$RECEIPT_CONTRACT_ID"  "$(field receipt_book)"
check "voucher.token is USDC"                    "$USDC_CONTRACT_ID"     "$(field token)"
check "voucher.admin is ADMIN_ADDRESS"           "$ADMIN_ADDRESS"        "$(field admin)"
check "voucher.fee_bps is $EXPECT_FEE_BPS"       "$EXPECT_FEE_BPS"       "$(field fee_bps)"
check "voucher.fee_account"                      "$EXPECT_FEE_ACCOUNT"   "$(field fee_account)"
check "voucher.dispute_window is ${EXPECT_WINDOW}s" "$EXPECT_WINDOW"     "$(field dispute_window)"

unquote() { jq -r '.' 2>/dev/null || true; }
check "receipt minter is the voucher"  "$VOUCHER_CONTRACT_ID" "$(view "$RECEIPT_CONTRACT_ID" get_minter | unquote)"
check "receipt admin is ADMIN_ADDRESS" "$ADMIN_ADDRESS"       "$(view "$RECEIPT_CONTRACT_ID" get_admin | unquote)"
check "registry admin is ADMIN_ADDRESS" "$ADMIN_ADDRESS"      "$(view "$REGISTRY_CONTRACT_ID" get_admin | unquote)"

if [ -n "${FAUCET_CONTRACT_ID:-}" ]; then
  faucet="$(view "$FAUCET_CONTRACT_ID" get_config)"
  check "faucet.token is USDC" "$USDC_CONTRACT_ID" \
    "$([ -n "$faucet" ] && printf '%s' "$faucet" | jq -r '.token // empty' 2>/dev/null || true)"
fi

printf '\n%s passed, %s failed\n' "$passes" "$failures"
if [ "$failures" -gt 0 ]; then
  printf 'The deployment is not wired the way %s says.\n' "$ENV_FILE"
  exit 1
fi
printf 'The deployment is wired the way %s says.\n' "$ENV_FILE"
