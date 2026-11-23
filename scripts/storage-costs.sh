#!/usr/bin/env bash
# Prints the measured size and rent of every ledger entry the three core
# contracts store. The table in docs/protocol/costs.md must match these rows.
#
# Sizes are XDR lengths of the real LedgerEntry; rent is computed with the
# mainnet rent parameters documented in the SDK's CostEstimate snapshot.
set -euo pipefail

cd "$(dirname "$0")/.."

cargo test -p sci-voucher --lib costs -- --nocapture
