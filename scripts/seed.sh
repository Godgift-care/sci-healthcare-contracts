#!/usr/bin/env bash
#
# Seeds a deployed SCI Healthcare instance with a demo provider, service,
# attester and one full voucher lifecycle. Safe to run against testnet only.
#
# Usage:  ./scripts/seed.sh [network]
#
# Reads contract ids from deployments/<network>.env, which deploy.sh writes.

set -euo pipefail

NETWORK="${1:-testnet}"
ADMIN="${ADMIN:-sci-admin}"
ISSUER="${ISSUER:-sci-issuer}"
CLINIC_ID="${CLINIC_ID:-sci-clinic}"
ATTESTER_ID="${ATTESTER_ID:-sci-attester}"
FUNDER_ID="${FUNDER_ID:-sci-funder}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="$ROOT/deployments/$NETWORK.env"
[ -f "$ENV_FILE" ] || { echo "No deployment found at $ENV_FILE. Run deploy.sh first."; exit 1; }
# shellcheck disable=SC1090
set -a; source "$ENV_FILE"; set +a

N="--network $NETWORK"
ADMIN_ADDR="$(stellar keys address "$ADMIN")"
ISSUER_ADDR="$(stellar keys address "$ISSUER")"
CLINIC="$(stellar keys address "$CLINIC_ID")"
ATTESTER="$(stellar keys address "$ATTESTER_ID")"
FUNDER="$(stellar keys address "$FUNDER_ID")"

say() { printf '\n\033[1m==> %s\033[0m\n' "$1"; }
quiet() { grep -E "Event:|error" | head -2 || true; }

say "Establishing USDC trustlines"
for who in "$FUNDER_ID" "$CLINIC_ID" "$ADMIN"; do
  stellar tx new change-trust --source-account "$who" $N \
    --line "USDC:$ISSUER_ADDR" >/dev/null 2>&1 || echo "  ($who already trusts USDC)"
done

# Once deploy-faucet.sh has run, the faucet is the token admin and the
# issuer can no longer mint, so the funder draws from the faucet instead.
if [ -n "${FAUCET_CONTRACT_ID:-}" ]; then
  say "Drawing test USDC for the funder from the faucet"
  stellar contract invoke --id "$FAUCET_CONTRACT_ID" --source-account "$FUNDER_ID" $N \
    -- drip --to "$FUNDER" 2>&1 | quiet
else
  say "Minting 100 USDC to the funder"
  stellar contract invoke --id "$USDC_CONTRACT_ID" --source-account "$ISSUER" $N \
    -- mint --to "$FUNDER" --amount 1000000000 2>&1 | quiet
fi

say "Registering provider"
stellar contract invoke --id "$REGISTRY_CONTRACT_ID" --source-account "$CLINIC_ID" $N \
  -- register_provider --owner "$CLINIC" \
  --name "Ikeja General Clinic" --country "NG" 2>&1 | quiet

# ProviderStatus is a unit enum with explicit discriminants, so the CLI
# takes the integer: 0 Pending, 1 Active, 2 Suspended.
say "Activating provider"
stellar contract invoke --id "$REGISTRY_CONTRACT_ID" --source-account "$ADMIN" $N \
  -- set_provider_status --admin "$ADMIN_ADDR" --provider_addr "$CLINIC" --status 1 2>&1 | quiet

say "Listing services"
stellar contract invoke --id "$REGISTRY_CONTRACT_ID" --source-account "$CLINIC_ID" $N \
  -- upsert_service --provider_addr "$CLINIC" --code 101 \
  --label "Outpatient consult" --price 30000000 2>&1 | quiet
stellar contract invoke --id "$REGISTRY_CONTRACT_ID" --source-account "$CLINIC_ID" $N \
  -- upsert_service --provider_addr "$CLINIC" --code 202 \
  --label "Malaria rapid test" --price 10000000 2>&1 | quiet
stellar contract invoke --id "$REGISTRY_CONTRACT_ID" --source-account "$CLINIC_ID" $N \
  -- upsert_service --provider_addr "$CLINIC" --code 303 \
  --label "Antenatal visit" --price 50000000 2>&1 | quiet

say "Authorising attester"
stellar contract invoke --id "$REGISTRY_CONTRACT_ID" --source-account "$ADMIN" $N \
  -- add_attester --admin "$ADMIN_ADDR" --attester "$ATTESTER" 2>&1 | quiet

# BytesN is passed as raw hex, not JSON. This stands in for the HMAC the
# client computes; no patient identifier ever reaches the chain.
BREF="$(printf 'patient-demo-001' | sha256sum | cut -d' ' -f1)"
EXPIRY=$(( $(date +%s) + 30*24*60*60 ))

say "Funding a voucher (3.00 USDC outpatient consult)"
stellar contract invoke --id "$VOUCHER_CONTRACT_ID" --source-account "$FUNDER_ID" $N \
  -- create_voucher --funder "$FUNDER" --beneficiary_ref "$BREF" \
  --provider "$CLINIC" --service_code 101 --amount 30000000 \
  --expires_at "$EXPIRY" 2>&1 | quiet

say "Clinic claims the voucher"
stellar contract invoke --id "$VOUCHER_CONTRACT_ID" --source-account "$CLINIC_ID" $N \
  -- claim --provider "$CLINIC" --voucher_id 1 2>&1 | quiet

say "Attester confirms delivery"
stellar contract invoke --id "$VOUCHER_CONTRACT_ID" --source-account "$ATTESTER_ID" $N \
  -- attest --attester "$ATTESTER" --voucher_id 1 2>&1 | quiet

# A second voucher is left Funded so the UI has an in-flight example.
say "Funding a second voucher, left in flight"
stellar contract invoke --id "$VOUCHER_CONTRACT_ID" --source-account "$FUNDER_ID" $N \
  -- create_voucher --funder "$FUNDER" --beneficiary_ref "$BREF" \
  --provider "$CLINIC" --service_code 202 --amount 10000000 \
  --expires_at "$EXPIRY" 2>&1 | quiet

cat <<DONE

Seeded. Voucher 1 is Attested and settles once the dispute window closes:

  stellar contract invoke --id $VOUCHER_CONTRACT_ID \\
    --source-account $ADMIN $N -- settle --voucher_id 1

Beneficiary reference used: $BREF
DONE
