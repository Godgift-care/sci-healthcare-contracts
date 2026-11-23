#!/usr/bin/env bash
# Regenerate contracts/<crate>/fixtures/stored_types/*.hex from the current types.
#
# Run this only at release time, after deciding that the storage layout change
# is deliberate and that old entries will be migrated or handled.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

crates=(registry voucher receipt sponsor faucet)
test_name="stored_type_fixtures_decode_against_current_code"

for crate in "${crates[@]}"; do
    echo "refreshing contracts/${crate}/fixtures/stored_types"
    UPDATE_STORED_TYPE_FIXTURES=1 cargo test -p "sci-${crate}" --lib "$test_name"
done
