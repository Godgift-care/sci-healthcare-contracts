#!/usr/bin/env bash
#
# Hands the admin role of registry, voucher and receipt to a new key, the two
# steps every contract requires: the current admin proposes, the new admin
# accepts.
#
# Usage:  ./scripts/rotate-admin.sh <network> <new-admin-identity> [--dry-run]
#
#   network              testnet, mainnet, local, ... (reads deployments/<network>.env)
#   new-admin-identity   a `stellar keys` identity holding the new admin key;
#                        it signs the accept_admin calls
#   --dry-run            print every command without sending a transaction
#
# Environment:
#   ADMIN   identity of the current admin (default: sci-admin)
#
# Order of work:
#
#   1. Preflight: read each contract's admin and refuse unless it is $ADMIN
#      or already the new admin (a re-run after a partial rotation).
#   2. propose_admin on every contract still to rotate.
#   3. accept_admin on each, signed by the new key.
#   4. Read each admin back, print a summary, and record the new address as
#      ADMIN_ADDRESS in deployments/<network>.env.
#
# It stops at the first failed call. Nothing changes until accept_admin, so a
# stop after proposing leaves the old admin in charge, and running the script
# again picks up where it stopped. The faucet is not rotated: it is
# testnet-only and has its own admin.
#
# What breaks if this is wrong: a contract left with the old admin while the
# others move, so an admin call that works on two contracts fails on the
# third in the middle of an incident.

set -euo pipefail

NETWORK=""
NEW_ID=""
DRY_RUN=0
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=1 ;;
    -h|--help) sed -n '2,33p' "${BASH_SOURCE[0]}"; exit 0 ;;
    -*) printf 'error: unknown option %s\n' "$arg" >&2; exit 2 ;;
    *)
      if [ -z "$NETWORK" ]; then NETWORK="$arg"
      elif [ -z "$NEW_ID" ]; then NEW_ID="$arg"
      else printf 'error: unexpected argument %s\n' "$arg" >&2; exit 2
      fi ;;
  esac
done
[ -n "$NETWORK" ] && [ -n "$NEW_ID" ] || {
  printf 'usage: %s <network> <new-admin-identity> [--dry-run]\n' "$0" >&2
  exit 2
}

ADMIN="${ADMIN:-sci-admin}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="$ROOT/deployments/$NETWORK.env"

say() { printf '\n\033[1m==> %s\033[0m\n' "$1"; }
die() { printf 'error: %s\n' "$1" >&2; exit 1; }

command -v stellar >/dev/null || die "the stellar CLI is required: https://github.com/stellar/stellar-cli"
command -v jq >/dev/null || die "jq is required"
[ -f "$ENV_FILE" ] || die "no deployment at $ENV_FILE"
# shellcheck disable=SC1090
set -a; source "$ENV_FILE"; set +a

ADMIN_ADDR="$(stellar keys address "$ADMIN")" || die "no identity named $ADMIN (set ADMIN to the current admin's identity)"
NEW_ADDR="$(stellar keys address "$NEW_ID")" || die "no identity named $NEW_ID"
[ "$ADMIN_ADDR" != "$NEW_ADDR" ] || die "$NEW_ID is already the admin key $ADMIN_ADDR"

# name:id-variable. Each contract's admin is read the way that contract exposes it.
CONTRACTS=(registry:REGISTRY_CONTRACT_ID voucher:VOUCHER_CONTRACT_ID receipt:RECEIPT_CONTRACT_ID)

view() { # <contract-id> <function> -> raw JSON result
  stellar contract invoke --id "$1" --source-account "$ADMIN" --network "$NETWORK" --send=no -- "$2"
}

admin_of() { # <name> <contract-id> -> G... address
  case "$1" in
    voucher) view "$2" get_config | jq -r '.admin' ;;
    *) view "$2" get_admin | jq -r '.' ;;
  esac
}

run() {
  printf '+ %s\n' "$*"
  [ "$DRY_RUN" -eq 1 ] || "$@" >/dev/null
}

say "Preflight on $NETWORK"
printf 'current admin: %s (%s)\nnew admin:     %s (%s)\n\n' "$ADMIN_ADDR" "$ADMIN" "$NEW_ADDR" "$NEW_ID"
TODO=()
for entry in "${CONTRACTS[@]}"; do
  name="${entry%%:*}"; var="${entry##*:}"; id="${!var:-}"
  [ -n "$id" ] || die "$var is not set in $ENV_FILE"
  current="$(admin_of "$name" "$id")" || die "could not read the $name admin from $id"
  if [ "$current" = "$NEW_ADDR" ]; then
    printf '  %-9s already rotated\n' "$name"
  elif [ "$current" = "$ADMIN_ADDR" ]; then
    printf '  %-9s admin %s\n' "$name" "$current"
    TODO+=("$entry")
  else
    die "the $name admin is $current, which is neither $ADMIN nor $NEW_ID; stopping before any change"
  fi
done

if [ "${#TODO[@]}" -gt 0 ]; then
  say "Proposing $NEW_ADDR"
  for entry in "${TODO[@]}"; do
    name="${entry%%:*}"; var="${entry##*:}"
    run stellar contract invoke --id "${!var}" --source-account "$ADMIN" --network "$NETWORK" \
      -- propose_admin --admin "$ADMIN_ADDR" --new_admin "$NEW_ADDR" \
      || die "propose_admin failed on $name; nothing has changed on it"
  done

  say "Accepting with $NEW_ID"
  for entry in "${TODO[@]}"; do
    name="${entry%%:*}"; var="${entry##*:}"
    run stellar contract invoke --id "${!var}" --source-account "$NEW_ID" --network "$NETWORK" \
      -- accept_admin --new_admin "$NEW_ADDR" \
      || die "accept_admin failed on $name; re-run to finish (the proposal stands)"
  done
fi

if [ "$DRY_RUN" -eq 1 ]; then
  say "Dry run complete — no transactions were sent"
  exit 0
fi

say "Verifying"
failures=0
printf '%-9s %-58s %s\n' CONTRACT ADMIN RESULT
for entry in "${CONTRACTS[@]}"; do
  name="${entry%%:*}"; var="${entry##*:}"
  current="$(admin_of "$name" "${!var}")"
  if [ "$current" = "$NEW_ADDR" ]; then result=ok; else result=FAILED; failures=$((failures + 1)); fi
  printf '%-9s %-58s %s\n' "$name" "$current" "$result"
done
[ "$failures" -eq 0 ] || die "$failures contract(s) do not report the new admin"

if grep -q '^ADMIN_ADDRESS=' "$ENV_FILE"; then
  sed -i.bak "s/^ADMIN_ADDRESS=.*/ADMIN_ADDRESS=$NEW_ADDR/" "$ENV_FILE" && rm -f "$ENV_FILE.bak"
else
  printf 'ADMIN_ADDRESS=%s\n' "$NEW_ADDR" >> "$ENV_FILE"
fi
printf '\nAll three contracts are administered by %s.\n' "$NEW_ADDR"
printf 'Updated ADMIN_ADDRESS in %s; commit it, and use ADMIN=%s from now on.\n' "$ENV_FILE" "$NEW_ID"
