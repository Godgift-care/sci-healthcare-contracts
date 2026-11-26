---
title: TTL policy
nav_order: 16
---

# TTL policy

Soroban stores two kinds of storage entries: **instance** (small, shared by the
whole contract — config, admin, counters) and **persistent** (per-record —
clinic profiles, vouchers, receipt indices). Both expire if nothing extends
them. This page explains how the shared constants in `contracts/common` govern
those lifetimes.

## Constants

| Constant | Ledgers | ≈ Days | Purpose |
|---|---|---|---|
| `DAY_IN_LEDGERS` | 17 280 | 1 | One calendar day at ~5-second ledger close times. |
| `INSTANCE_TTL_THRESHOLD` | 120 960 | 7 | Re-extend instance storage when TTL falls below this. |
| `INSTANCE_TTL_EXTEND` | 518 400 | 30 | Target TTL when extending instance storage. |
| `PERSISTENT_TTL_THRESHOLD` | 518 400 | 30 | Re-extend a persistent entry when TTL falls below this. |
| `PERSISTENT_TTL_EXTEND` | 1 555 200 | 90 | Target TTL when extending a persistent entry. |
| `ATTESTATIONS_TTL` | 34 560 | 2 | Fixed TTL for daily attestation counters. |

The day figures are derived from the constants in
[`contracts/common/src/lib.rs`](../../contracts/common/src/lib.rs) and pinned
by a test (`ttl_doc_tests::ttl_doc_figures_match_constants`) that fails if a
constant changes without updating this document.

## Which entries use which TTL

### Instance storage

Holds contract configuration, the admin address, running totals (open escrow,
voucher counter, per-provider counts). Every call to any writable entry point
calls `extend_instance`, so a live contract replenishes itself automatically.

Threshold: **7 days**. Extend to: **30 days**.

An idle testnet deployment (no calls for more than 7 days) will archive its
instance. The weekly `scripts/extend-ttl.sh` / [extend-ttl workflow](../../.github/workflows/extend-ttl.yml)
prevents this.

### Persistent storage

Holds per-clinic registry entries, individual vouchers, receipt records and
per-clinic receipt indices. Each is extended on read **and** write via
`extend_persistent`, so a record accessed regularly stays alive indefinitely.

Threshold: **30 days**. Extend to: **90 days**.

An idle record (not read or written for 30 days) archives. It can be restored
with `stellar contract restore` before the hash is purged from the ledger.

### Attestation counters

Track how many times a beneficiary has been attested per UTC day. Two days
covers whatever hour on that day the counter was first written, with a one-day
margin.

Fixed TTL: **2 days** (not threshold-based; always set on write).

## What happens when an entry archives

An archived entry is removed from the working set but its hash is still on
chain. It can be restored with:

```bash
stellar contract restore --id <CONTRACT_ID> --key <KEY> --network <NETWORK> \
  --source-account <ADMIN>
```

After a restore, the next read or write extends it back to the normal target.
`extend_instance` and `extend_persistent` will not rescue an already-archived
entry — restore must come first.

For instance storage, any entry-point call restores and re-extends it in one
step because every writable function calls `extend_instance`.

## Keeping contracts alive on testnet

The `scripts/extend-ttl.sh` script extends the instance TTL and the uploaded
wasm code for every contract in `deployments/testnet.env`. Run it weekly or
let the [extend-ttl workflow](../../.github/workflows/extend-ttl.yml) do it on
a schedule.

```bash
./scripts/extend-ttl.sh testnet
```

## Changing the constants

1. Edit `contracts/common/src/lib.rs`.
2. The test `ttl_doc_tests::ttl_doc_figures_match_constants` will fail until
   you also update the **Ledgers** and **≈ Days** columns in the table above.
3. Update any downstream documentation that cites specific day counts.
