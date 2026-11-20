#!/usr/bin/env bash
# Regenerate contracts/<crate>/storage-counts.md from each crate's AUTH_MATRIX.
#
# Run this only when a read or write count change is intentional; the committed
# table is what the test compares against.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

crates=(registry voucher receipt sponsor faucet)
test_name="storage_counts_match_the_committed_table"

for crate in "${crates[@]}"; do
    echo "refreshing contracts/${crate}/storage-counts.md"
    UPDATE_STORAGE_COUNTS=1 cargo test -p "sci-${crate}" --lib "$test_name"
done
