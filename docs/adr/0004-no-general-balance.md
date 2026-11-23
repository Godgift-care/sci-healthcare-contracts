---
title: "0004 — The contract holds no general balance"
parent: Architecture decision records
nav_order: 4
---

# 0004 — The contract holds no general balance

- **Status:** Accepted
- **Date:** 2026-11-20
- **Decision area:** contracts

## Context

The obvious design for "pay for care later" is a wallet: funders deposit, the
protocol keeps a balance per address, and a clinic withdraws what it earned.
Every exchange, every custodial payment app and every layer-2 bridge is built
that way, so it is what a new contributor expects to find.

It is also a different product. A stored balance can be spent on anything the
contract later allows, so purpose-binding becomes a UI promise rather than a
contract rule. It makes the protocol a custodian of other people's money, which
brings withdrawal queues, solvency risk, key-custody requirements and a
regulatory posture that this project deliberately does not have. And once a
general balance exists, "send it anywhere" and "refund to a different address"
are one feature request away.

The protocol's whole claim is narrower: money arrives already attached to one
clinic, one service and one expiry, and leaves in exactly two directions.

## Decision

The voucher contract has no deposit, no per-user balance, no withdrawal and no
sweep. Every token movement is tied to a voucher id:

- [`contracts/voucher/src/lib.rs` — `VoucherEscrow::fund`](https://github.com/Godgift-care/sci-healthcare-contracts/blob/main/contracts/voucher/src/lib.rs#L902)
  moves the amount from the funder straight into the contract in the same call
  that writes the voucher; there is no state where the money sits unattached.
- [`contracts/voucher/src/lib.rs` — `VoucherEscrow::pay_out`](https://github.com/Godgift-care/sci-healthcare-contracts/blob/main/contracts/voucher/src/lib.rs#L1183)
  sends the net to the stored provider, the attester share to the stored
  attester and the fee to the configured fee account — all three derived from
  the voucher and config, not from the caller.
- [`contracts/voucher/src/lib.rs` — `VoucherEscrow::pay_back`](https://github.com/Godgift-care/sci-healthcare-contracts/blob/main/contracts/voucher/src/lib.rs#L1286)
  sends the full amount to the stored funder, with no fee.
- The only counter is an accounting total, not a spendable asset:
  [`contracts/voucher/src/lib.rs` — `VoucherEscrow::escrow_summary`](https://github.com/Godgift-care/sci-healthcare-contracts/blob/main/contracts/voucher/src/lib.rs#L881)
  returns `(open_escrow, token_balance)`, maintained by
  [`contracts/voucher/src/storage.rs` — `increase_open_escrow` / `decrease_open_escrow`](https://github.com/Godgift-care/sci-healthcare-contracts/blob/main/contracts/voucher/src/storage.rs#L146).
  It has no setter and is never consulted to pay anyone.
- The rule appears in the crate documentation as the first design statement:
  [`contracts/voucher/src/lib.rs` — module docs, "Why purpose-bound"](https://github.com/Godgift-care/sci-healthcare-contracts/blob/main/contracts/voucher/src/lib.rs#L1).
- The absence is tested as an invariant, not just documented:
  `escrow_summary_tracks_the_settlement_lifecycle`,
  `escrow_summary_tracks_a_refund` and
  `escrow_summary_tracks_each_voucher_and_dispute_resolution` in
  [`contracts/voucher/src/test.rs`](https://github.com/Godgift-care/sci-healthcare-contracts/blob/main/contracts/voucher/src/test.rs#L167)
  require `open_escrow == token_balance` after every operation.

## Consequences

- Funds can only ever reach the provider named on the voucher, the funder who
  paid, and the protocol fee/attester split carved out of the provider's
  amount. A compromised admin key cannot drain a float, because there is no
  float: `resolve_dispute` and `resolve_split` can only route one voucher's
  escrow between its own two parties.
- The contract is not a wallet. A funder with $30 for three visits creates
  three vouchers, possibly at different clinics, rather than holding a balance.
  That is the intended shape, and clients must present it that way.
- There is no way to reuse a refunded voucher: the money returns to the funder
  and a new voucher must be created. Integrators cannot "top up" an existing
  voucher; they create another one.
- `escrow_summary` can legitimately show `token_balance > open_escrow` if
  someone sends tokens to the contract address directly. That is a donation,
  not a credit: no function will ever pay it out. The view exists to detect
  accounting bugs, not to promise a balance.
- Because funds are never pooled, per-voucher storage is the cost model. Rent
  scales with the number of vouchers rather than with an account balance; see
  [Storage and rent costs](../protocol/costs.md).
- Any proposal to add `deposit`, `withdraw`, `transfer_balance` or a
  "refund to a different address" changes the product and the trust model, and
  needs a superseding ADR — not a quiet PR.

## Alternatives rejected

- **Per-funder balance with `withdraw`.** Reintroduces custody, withdrawal
  liveness, solvency accounting and the ability to spend money on care other
  than the one it was sent for. It also makes the protocol an e-money issuer in
  most jurisdictions.
- **Per-patient balance, drawn down by clinics.** Slightly better for the
  patient, but it still stores value without a purpose and makes every clinic
  claim a debit against a shared pot. Disputes become balance disputes, which
  are harder to bound than "this voucher, this clinic".
- **A single escrow pot keyed by voucher id with a sweep function.** The sweep
  is the general balance by another name: one call that moves unattached funds
  needs an authorisation rule, and that rule is the custodian.
- **Hold funds in a separate escrow account per voucher.** It multiplies
  accounts and key management, and Soroban contract storage already gives each
  voucher an isolated entry. The token balance is pooled at the address level,
  but `open_escrow` and per-voucher accounting keep it non-spendable.
- **Use a generic escrow contract and keep this one thin.** A generic escrow
  exposes release-to-anyone and arbitrary-arbiter functions. Narrowing it back
  to two recipients is the work, and the result is this contract with more
  configuration to get wrong.
