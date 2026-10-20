#!/usr/bin/env bash
#
# Seeds a deployed SCI Healthcare instance with realistic volume for demos
# and load tests: many clinics in several countries, and vouchers spread
# across every state a voucher can be in. Testnet or a local node only.
#
# Usage:  ./scripts/seed-large.sh <network> <clinics> <vouchers>
#
#   clinics   how many clinics to register and activate (1-99)
#   vouchers  how many vouchers to fund across them
#
# Environment:
#   ADMIN, ISSUER, ATTESTER_ID   identities, as for seed.sh (default sci-admin,
#                                sci-issuer, sci-attester)
#   PREFIX        prefix for the identities this script creates (default sci-large)
#   SHORT_EXPIRY  seconds of life for vouchers that end Refunded (default 30)
#   MAX_WAIT      most seconds to wait for expiries and the dispute window
#                 before settling and refunding (default 300)
#
# Identity creation is idempotent: clinics and funders are named
# $PREFIX-clinic-N / $PREFIX-funder-N and reused if they exist, so a re-run
# with the same counts adds vouchers to the same clinics. Registering a clinic
# that is already registered is skipped.
#
# Vouchers cycle through the six states in order — Funded, Claimed, Attested,
# Settled, Disputed, Refunded. A voucher can be Settled only after the
# deployment's dispute window and Refunded only after its expiry; the script
# waits up to MAX_WAIT for both and leaves any it could not finish Attested or
# Funded, saying so. For a full spread on a local node, deploy with a short
# window: DISPUTE_WINDOW=30 ./scripts/deploy.sh local.

set -euo pipefail

[ $# -eq 3 ] || { printf 'usage: %s <network> <clinics> <vouchers>\n' "$0" >&2; exit 2; }
NETWORK="$1"
CLINICS="$2"
VOUCHERS="$3"
case "$CLINICS$VOUCHERS" in *[!0-9]*|'') printf 'error: counts must be whole numbers\n' >&2; exit 2 ;; esac
[ "$CLINICS" -ge 1 ] && [ "$CLINICS" -le 99 ] || { printf 'error: clinics must be 1-99\n' >&2; exit 2; }
[ "$VOUCHERS" -ge 0 ] || { printf 'error: vouchers must be 0 or more\n' >&2; exit 2; }
[ "$NETWORK" != "mainnet" ] || { printf 'error: seed-large.sh never runs against mainnet\n' >&2; exit 2; }

ADMIN="${ADMIN:-sci-admin}"
ISSUER="${ISSUER:-sci-issuer}"
ATTESTER_ID="${ATTESTER_ID:-sci-attester}"
PREFIX="${PREFIX:-sci-large}"
SHORT_EXPIRY="${SHORT_EXPIRY:-30}"
MAX_WAIT="${MAX_WAIT:-300}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="$ROOT/deployments/$NETWORK.env"
[ -f "$ENV_FILE" ] || { echo "No deployment found at $ENV_FILE. Run deploy.sh first."; exit 1; }
# shellcheck disable=SC1090
set -a; source "$ENV_FILE"; set +a
command -v jq >/dev/null || { echo "jq is required"; exit 1; }

say() { printf '\n\033[1m==> %s\033[0m\n' "$1"; }
inv() { # <contract-id> <source-identity> -- <function> [args...]
  local id="$1" src="$2"; shift 2
  stellar contract invoke --quiet --id "$id" --source-account "$src" --network "$NETWORK" "$@"
}

# Identities: create once, reuse afterwards.
ensure_identity() {
  if stellar keys address "$1" >/dev/null 2>&1; then
    # Reused, perhaps from another network: make sure it has an account here.
    stellar keys fund "$1" --network "$NETWORK" >/dev/null 2>&1 || true
  else
    stellar keys generate "$1" --network "$NETWORK" --fund >/dev/null
  fi
  stellar keys address "$1"
}

ADMIN_ADDR="$(stellar keys address "$ADMIN")"
ISSUER_ADDR="$(stellar keys address "$ISSUER")"
ATTESTER="$(ensure_identity "$ATTESTER_ID")"

trust_usdc() {
  stellar tx new change-trust --source-account "$1" --network "$NETWORK" \
    --line "USDC:$ISSUER_ADDR" >/dev/null 2>&1 || true
}

# Countries the protocol serves; clinics are spread across them in turn.
COUNTRIES=(NG KE GH UG TZ RW ZA SN ET CD)
NAMES=("General Clinic" "Community Health Centre" "Family Practice" "Maternity Home" "Diagnostics Lab")
# code:label:price (7 decimals). Vouchers use the cheapest, so a funder's
# test USDC stretches furthest.
SERVICES=("202:Malaria rapid test:10000000" "101:Outpatient consult:30000000" "303:Antenatal visit:50000000")
PRICE=10000000
CODE=202

say "Clinics: $CLINICS"
CLINIC_ADDRS=()
for n in $(seq 1 "$CLINICS"); do
  id="$PREFIX-clinic-$n"
  addr="$(ensure_identity "$id")"
  CLINIC_ADDRS+=("$addr")
  trust_usdc "$id"
  country="${COUNTRIES[$(( (n - 1) % ${#COUNTRIES[@]} ))]}"
  name="${NAMES[$(( (n - 1) % ${#NAMES[@]} ))]} $n"

  if inv "$REGISTRY_CONTRACT_ID" "$ADMIN" --send=no -- get_provider --provider_addr "$addr" >/dev/null 2>&1; then
    printf '  %-22s %s already registered\n' "$id" "$country"
  else
    inv "$REGISTRY_CONTRACT_ID" "$id" -- register_provider --owner "$addr" \
      --name "$name" --country "$country" >/dev/null
    printf '  %-22s %s registered\n' "$id" "$country"
  fi
  inv "$REGISTRY_CONTRACT_ID" "$ADMIN" -- set_provider_status --admin "$ADMIN_ADDR" \
    --provider_addr "$addr" --status 1 >/dev/null
  for svc in "${SERVICES[@]}"; do
    IFS=: read -r code label price <<<"$svc"
    inv "$REGISTRY_CONTRACT_ID" "$id" -- upsert_service --provider_addr "$addr" \
      --code "$code" --label "$label" --price "$price" >/dev/null
  done
done

say "Attester"
inv "$REGISTRY_CONTRACT_ID" "$ADMIN" -- add_attester --admin "$ADMIN_ADDR" --attester "$ATTESTER" >/dev/null
printf '  %s\n' "$ATTESTER"

# Funders: one per ten vouchers. Each is funded from the faucet when there is
# one (the issuer can no longer mint once deploy-faucet.sh has run), else
# minted by the issuer.
FUNDERS=$(( (VOUCHERS + 9) / 10 ))
[ "$FUNDERS" -ge 1 ] || FUNDERS=1
say "Funders: $FUNDERS"
FUNDER_IDS=()
for n in $(seq 1 "$FUNDERS"); do
  id="$PREFIX-funder-$n"
  addr="$(ensure_identity "$id")"
  FUNDER_IDS+=("$id")
  trust_usdc "$id"
  if [ -n "${FAUCET_CONTRACT_ID:-}" ]; then
    inv "$FAUCET_CONTRACT_ID" "$id" -- drip --to "$addr" >/dev/null 2>&1 \
      || printf '  %s: faucet cooldown, using its existing balance\n' "$id"
  else
    inv "$USDC_CONTRACT_ID" "$ISSUER" -- mint --to "$addr" --amount $((10 * PRICE)) >/dev/null
  fi
  printf '  %s %s\n' "$id" "$addr"
done

STATES=(Funded Claimed Attested Settled Disputed Refunded)
declare -A COUNT=()
TO_SETTLE=()
TO_REFUND=()
now() { date +%s; }

say "Vouchers: $VOUCHERS"
for v in $(seq 1 "$VOUCHERS"); do
  state="${STATES[$(( (v - 1) % ${#STATES[@]} ))]}"
  funder_id="${FUNDER_IDS[$(( (v - 1) / 10 ))]}"
  funder="$(stellar keys address "$funder_id")"
  c=$(( (v - 1) % CLINICS ))
  clinic_id="$PREFIX-clinic-$((c + 1))"
  clinic="${CLINIC_ADDRS[$c]}"
  bref="$(printf '%s-patient-%s' "$PREFIX" "$v" | sha256sum | cut -d' ' -f1)"
  if [ "$state" = Refunded ]; then expiry=$(( $(now) + SHORT_EXPIRY )); else expiry=$(( $(now) + 30*24*60*60 )); fi

  vid="$(inv "$VOUCHER_CONTRACT_ID" "$funder_id" -- create_voucher --funder "$funder" \
    --beneficiary_ref "$bref" --provider "$clinic" --service_code "$CODE" \
    --amount "$PRICE" --expires_at "$expiry" | tr -d '"')"

  case "$state" in
    Claimed|Attested|Settled|Disputed)
      inv "$VOUCHER_CONTRACT_ID" "$clinic_id" -- claim --provider "$clinic" --voucher_id "$vid" >/dev/null ;;
  esac
  case "$state" in
    Attested|Settled)
      inv "$VOUCHER_CONTRACT_ID" "$ATTESTER_ID" -- attest --attester "$ATTESTER" --voucher_id "$vid" >/dev/null ;;
    Disputed)
      inv "$VOUCHER_CONTRACT_ID" "$funder_id" -- dispute --funder "$funder" --voucher_id "$vid" --reason_code 1 >/dev/null ;;
  esac
  [ "$state" = Settled ] && TO_SETTLE+=("$vid")
  [ "$state" = Refunded ] && TO_REFUND+=("$vid")
  printf '  voucher %-5s %-9s %s\n' "$vid" "$state" "$clinic_id"
done

# Settled and Refunded need time to pass. Wait for the later of the two, up
# to MAX_WAIT, then finish whatever is due.
if [ "${#TO_SETTLE[@]}" -gt 0 ] || [ "${#TO_REFUND[@]}" -gt 0 ]; then
  window="$(inv "$VOUCHER_CONTRACT_ID" "$ADMIN" --send=no -- get_config | jq -r '.dispute_window')"
  wait_for=0
  [ "${#TO_REFUND[@]}" -gt 0 ] && wait_for=$((SHORT_EXPIRY + 5))
  [ "${#TO_SETTLE[@]}" -gt 0 ] && [ "$((window + 5))" -gt "$wait_for" ] && wait_for=$((window + 5))
  if [ "$wait_for" -gt "$MAX_WAIT" ]; then wait_for=$((SHORT_EXPIRY + 5)); fi
  say "Waiting ${wait_for}s for expiries and the dispute window (${window}s)"
  sleep "$wait_for"
fi

finished_settles=0
for vid in "${TO_SETTLE[@]}"; do
  if inv "$VOUCHER_CONTRACT_ID" "$ADMIN" -- settle --voucher_id "$vid" >/dev/null 2>&1; then
    finished_settles=$((finished_settles + 1))
  fi
done
finished_refunds=0
for vid in "${TO_REFUND[@]}"; do
  if inv "$VOUCHER_CONTRACT_ID" "$ADMIN" -- refund --voucher_id "$vid" >/dev/null 2>&1; then
    finished_refunds=$((finished_refunds + 1))
  fi
done

say "Summary"
# Read every voucher back from the chain rather than trusting the plan.
declare -A STATUS_NAME=([0]=Funded [1]=Claimed [2]=Attested [3]=Settled [4]=Disputed [5]=Refunded)
# A failed read here is reported, not fatal: the seeding itself is done.
last="$(inv "$VOUCHER_CONTRACT_ID" "$ADMIN" --send=no -- next_voucher_id | tr -d '"')" || last=1
first=$(( last - VOUCHERS ))
for vid in $(seq "$first" $((last - 1))); do
  [ "$vid" -ge 1 ] || continue
  s="$(inv "$VOUCHER_CONTRACT_ID" "$ADMIN" --send=no -- get_voucher --voucher_id "$vid" | jq -r '.status')" \
    || s="unread"
  name="${STATUS_NAME[$s]:-$s}"
  COUNT[$name]=$(( ${COUNT[$name]:-0} + 1 ))
done
printf '%s clinics in %s countries\n' "$CLINICS" "$(( CLINICS < ${#COUNTRIES[@]} ? CLINICS : ${#COUNTRIES[@]} ))"
for state in "${STATES[@]}"; do printf '  %-9s %s\n' "$state" "${COUNT[$state]:-0}"; done
if [ "$finished_settles" -lt "${#TO_SETTLE[@]}" ]; then
  printf '\n%s voucher(s) meant to settle are still Attested: the dispute window (%ss)\n' \
    "$(( ${#TO_SETTLE[@]} - finished_settles ))" "$window"
  printf 'is longer than MAX_WAIT (%ss). Settle them later with:\n' "$MAX_WAIT"
  printf '  stellar contract invoke --id %s --source-account %s --network %s -- settle --voucher_id <id>\n' \
    "$VOUCHER_CONTRACT_ID" "$ADMIN" "$NETWORK"
fi
if [ "$finished_refunds" -lt "${#TO_REFUND[@]}" ]; then
  printf '\n%s voucher(s) meant to refund are still Funded.\n' "$(( ${#TO_REFUND[@]} - finished_refunds ))"
fi
