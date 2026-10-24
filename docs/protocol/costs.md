---
title: Storage and rent costs
nav_order: 4
---

# Storage and rent costs

Every object the protocol stores is a Soroban ledger entry, and every live
entry rents space for as long as it is kept. This page lists the size of each
entry and what its configured TTL costs at the network's rent rate, so an
operator or grant reviewer can see what running the protocol actually costs.

The numbers are measured, not estimated by hand:
`contracts/voucher/src/test/costs.rs` builds one of every entry in the test
host, XDR-encodes the real `LedgerEntry` and computes rent with integer math.
The same test fails when this table drifts from the code, and
`scripts/storage-costs.sh` prints the measurements again.

## The method

Rent on Stellar is charged per 1 KB of ledger space per ledger it is kept:

```text
rent = ceil(size_bytes × fee_per_rent_1kb × ttl_ledgers / (1024 × rent_rate_denominator))
```

| Parameter | Persistent | Temporary |
| --- | --- | --- |
| `fee_per_rent_1kb` | 12,000 stroops | 12,000 stroops |
| `rent_rate_denominator` | 1,215 ledgers | 2,430 ledgers |
| Configured TTL | 90 days = 1,555,200 ledgers | 2 days = 34,560 ledgers |
| Contract instance TTL | 30 days = 518,400 ledgers | — |

One day is 17,280 ledgers at roughly five seconds per ledger, and 1 XLM is
10,000,000 stroops. The rent parameters are the mainnet snapshot in
`soroban-sdk` 27.0.6's `CostEstimate` (dated 2026-07-10); the SDK documents
12,000 stroops/KB as a deliberately conservative overestimate, so these
figures are an upper bound rather than a quote. Check the live values with
`stellar network settings --network mainnet` when planning a launch.

Sizes below are the XDR length of the `LedgerEntry`, which is what Stellar
uses for rent. Variable-length fields make an entry bigger: the provider row
assumes the name `Ikeja General Clinic` and country `NG`, and the service row
assumes the label `Outpatient consult`. A longer name or label adds its extra
bytes to that entry and its rent.

## Per-object costs

| Contract | Entry | Durability | TTL (ledgers) | Size (bytes) | Rent (XLM) | Rent (stroops) |
| --- | --- | --- | --- | --- | --- | --- |
| `registry` | contract instance (`Admin`) | persistent | 518400 | 176 | 0.0880000 | 880000 |
| `registry` | `Provider(owner)` | persistent | 1555200 | 320 | 0.4800000 | 4800000 |
| `registry` | `Service(provider, code)` | persistent | 1555200 | 372 | 0.5580000 | 5580000 |
| `registry` | `Attester(address)` | persistent | 1555200 | 132 | 0.1980000 | 1980000 |
| `voucher` | contract instance (`Config`, `NextId`, `OpenEscrow`, `AttestationCap`) | persistent | 518400 | 668 | 0.3340000 | 3340000 |
| `voucher` | `Voucher(id)` | persistent | 1555200 | 328 | 0.4920000 | 4920000 |
| `voucher` | `Attestations(attester, day)` | temporary | 34560 | 148 | 0.0024667 | 24667 |
| `receipt` | contract instance (`Admin`, `Minter`) | persistent | 518400 | 244 | 0.1220000 | 1220000 |
| `receipt` | `Receipt(voucher_id)` | persistent | 1555200 | 356 | 0.5340000 | 5340000 |
| `receipt` | `Count(beneficiary_ref)` | persistent | 1555200 | 132 | 0.1980000 | 1980000 |
| `receipt` | `ProviderCount(provider)` | persistent | 1555200 | 140 | 0.2100000 | 2100000 |
| `receipt` | `ProviderReceipt(provider, index)` | persistent | 1555200 | 152 | 0.2280000 | 2280000 |

The contract instance is one ledger entry shared by every key kept in instance
storage. `PendingAdmin` (registry and voucher) and `PendingMinter` (receipt)
live in that same entry, so a handover in progress makes it a little larger
rather than creating a separate entry. The table shows the ordinary state with
no handover pending.

`OpenEscrow` and `NextId` are written by `create_voucher`; `AttestationCap` is
written by the admin, and the instance row above includes it. A deployment
that never calls `set_attestation_cap` has a slightly smaller voucher instance.

## What a voucher costs

A funded voucher keeps one `Voucher(id)` entry alive for 90 days: **0.492 XLM**
at the parameters above. Settling it adds a `Receipt(voucher_id)`, a
`Count(beneficiary_ref)` and two provider-index entries in the receipt book
(0.534 + 0.198 + 0.210 + 0.228 = **1.17 XLM**), plus one temporary
`Attestations` entry (**0.0024667 XLM**) that disappears after two days.

The registry side is shared: one `Provider` entry and one `Service` entry per
clinic and price list, not per voucher. An attester entry is one per attester.

These are storage costs, not transaction fees. Every lifecycle call also pays
the normal Stellar resource fee (base fee plus CPU, read/write and event
bytes); [Economics](economics.md#network-costs) covers those. Rent is the part
that is easy to forget because it is paid when an entry is created or its TTL
is extended, not as a separate line on a transaction.

## Reproducing the numbers

```bash
./scripts/storage-costs.sh
```

The script runs the measurement test with `--nocapture`. Each line has the
contract, entry, durability, TTL, size and rent in stroops. Change a struct or
a TTL and the test fails until this table is regenerated, which is deliberate:
a silent change in running cost should be visible in review.
