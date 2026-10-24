#!/usr/bin/env bash
#
# Checks that the contracts deployed on a network are the ones this
# repository builds.
#
# Usage:  ./scripts/verify-deployments.sh [network] [options]
#
#   network        testnet (default) or mainnet
#   --wasm-dir D   compare against the wasm files in D instead of building
#                  (use it with the artefacts downloaded from a release)
#   --skip-build   reuse the wasm files already in --wasm-dir
#
# For every contract in deployments/<network>.env it fetches the deployed
# wasm with `stellar contract fetch` and compares it with the local build:
#
#   hash      SHA-256 of the whole file. Equal hashes mean the deployed
#             bytes are the bytes this source produces.
#   interface the contract interface (functions, types, errors, events) as
#             the Stellar host sees it. Equal interfaces mean the deployed
#             contract behaves like the source even if the bytes differ.
#
# The script exits non-zero when a contract does not match.
#
# What breaks if this is wrong: the contracts on chain are not the ones in
# this repository, and the claim in the README is false.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

NETWORK=""
WASM_DIR=""
SKIP_BUILD=0

while [ $# -gt 0 ]; do
  case "$1" in
    --wasm-dir)
      [ $# -ge 2 ] || { printf 'error: --wasm-dir needs a directory\n' >&2; exit 2; }
      WASM_DIR="$2"; shift 2 ;;
    --wasm-dir=*) WASM_DIR="${1#--wasm-dir=}"; shift ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    -h|--help) sed -n '2,25p' "${BASH_SOURCE[0]}"; exit 0 ;;
    -*) printf 'error: unknown option %s\n' "$1" >&2; exit 2 ;;
    *)
      if [ -z "$NETWORK" ]; then NETWORK="$1"; else printf 'error: unexpected argument %s\n' "$1" >&2; exit 2; fi
      shift ;;
  esac
done
NETWORK="${NETWORK:-testnet}"

ENV_FILE="$ROOT/deployments/$NETWORK.env"
[ -f "$ENV_FILE" ] || { printf 'error: no deployment file at %s\n' "$ENV_FILE" >&2; exit 1; }
# shellcheck disable=SC1090
set -a; source "$ENV_FILE"; set +a

command -v stellar >/dev/null || { printf 'error: the stellar CLI is required: https://github.com/stellar/stellar-cli\n' >&2; exit 1; }

say() { printf '\n\033[1m==> %s\033[0m\n' "$1"; }

WASM_DIR="${WASM_DIR:-$ROOT/dist}"
if [ "$SKIP_BUILD" -eq 0 ]; then
  say "Building the contracts to compare against"
  "$ROOT/scripts/build-wasm.sh" "$WASM_DIR" >/dev/null
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# contract id variable -> wasm file name
CONTRACTS=(
  "REGISTRY_CONTRACT_ID:sci_registry.wasm"
  "VOUCHER_CONTRACT_ID:sci_voucher.wasm"
  "RECEIPT_CONTRACT_ID:sci_receipt.wasm"
  "FAUCET_CONTRACT_ID:sci_faucet.wasm"
)

say "Comparing the deployed contracts with $WASM_DIR"
printf '%-10s %-12s %-12s %s\n' "CONTRACT" "HASH" "INTERFACE" "DETAIL"
checked=0
hash_matches=0
interface_matches=0
failures=0

for entry in "${CONTRACTS[@]}"; do
  var="${entry%%:*}"
  file="${entry##*:}"
  id="${!var:-}"
  [ -n "$id" ] || continue
  local_wasm="$WASM_DIR/$file"
  checked=$((checked + 1))
  if [ ! -f "$local_wasm" ]; then
    printf '%-10s %-12s %-12s %s\n' "${file#sci_}" "-" "-" "no local build at $local_wasm"
    failures=$((failures + 1))
    continue
  fi

  fetched="$TMP/$file"
  if ! stellar contract fetch --id "$id" --network "$NETWORK" --out-file "$fetched" >/dev/null 2>&1; then
    printf '%-10s %-12s %-12s %s\n' "${file#sci_}" "-" "-" "could not fetch $id"
    failures=$((failures + 1))
    continue
  fi

  deployed_hash="$(sha256sum "$fetched" | cut -d' ' -f1)"
  local_hash="$(sha256sum "$local_wasm" | cut -d' ' -f1)"
  if [ "$deployed_hash" = "$local_hash" ]; then
    hash_state="match"
    hash_detail=""
    hash_matches=$((hash_matches + 1))
  else
    hash_state="differ"
    hash_detail="deployed ${deployed_hash:0:12}… local ${local_hash:0:12}…"
    failures=$((failures + 1))
  fi

  interface_state="-"
  interface_detail=""
  deployed_iface="$TMP/$file.iface.json"
  local_iface="$TMP/$file.local.iface.json"
  if stellar contract info interface --contract-id "$id" --network "$NETWORK" --output json >"$deployed_iface" 2>/dev/null \
     && stellar contract info interface --wasm "$local_wasm" --output json >"$local_iface" 2>/dev/null; then
    if diff -q "$deployed_iface" "$local_iface" >/dev/null 2>&1; then
      interface_state="match"
      interface_matches=$((interface_matches + 1))
    else
      interface_state="differ"
      interface_detail="the deployed contract exposes a different interface"
      failures=$((failures + 1))
    fi
  else
    interface_detail="interface check unavailable (needs stellar CLI 22 or newer)"
  fi

  detail="$hash_detail"
  [ -n "$interface_detail" ] && detail="${detail:+$detail; }$interface_detail"
  printf '%-10s %-12s %-12s %s\n' "${file#sci_}" "$hash_state" "$interface_state" "$detail"
done

say "Verdict"
printf 'byte for byte: %s of %s contracts match\n' "$hash_matches" "$checked"
printf 'interface:     %s of %s contracts match\n' "$interface_matches" "$checked"
if [ "$failures" -eq 0 ]; then
  printf 'Every deployed contract is the one this source builds.\n'
  exit 0
fi
printf '\nThe deployed contracts are not the ones this source builds.\n'
exit 1
