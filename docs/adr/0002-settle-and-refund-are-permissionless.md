---
title: "0002 — Settle and refund are permissionless"
parent: Architecture decision records
nav_order: 2
---

# 0002 — Settle and refund are permissionless

- **Status:** Accepted
- **Date:** 2026-11-20
- **Decision area:** contracts

## Context

Two transitions move escrowed money to its final owner and neither changes who
owns it:

- `settle` pays a clinic for care an attester already confirmed.
- `refund` returns money to the funder when care can no longer happen.

Both are due at a time the chain can compute: `settle` after
`dispute_deadline`, `refund` at `refundable_at`. Nobody has a discretion to
exercise at that point — the state and the clock already decide.

If either call required the other party's signature, the party who owes the
money could simply not sign. A clinic would depend on the funder to release
funds the funder has already promised, and a funder would depend on a clinic to
return money for care that never happened. That is the failure the escrow
exists to prevent, reintroduced one layer up.

## Decision

`settle` and `refund` take no actor argument, call no `require_auth()`, and
enforce eligibility from stored state and ledger time only:

- [`contracts/voucher/src/lib.rs` — `VoucherEscrow::settle`](https://github.com/Godgift-care/sci-healthcare-contracts/blob/main/contracts/voucher/src/lib.rs#L553)
  requires status `Attested` and `timestamp >= dispute_deadline`, then pays
  through `pay_out`.
- [`contracts/voucher/src/lib.rs` — `VoucherEscrow::refund`](https://github.com/Godgift-care/sci-healthcare-contracts/blob/main/contracts/voucher/src/lib.rs#L581)
  compares the clock with `refundable_at_for` and pays through `pay_back`.
- [`contracts/voucher/src/lib.rs` — `VoucherEscrow::refundable_at`](https://github.com/Godgift-care/sci-healthcare-contracts/blob/main/contracts/voucher/src/lib.rs#L818)
  publishes the same deadline as a view, so a keeper can plan without
  simulating.
- [`contracts/voucher/src/lib.rs` — `VoucherEscrow::pay_out`](https://github.com/Godgift-care/sci-healthcare-contracts/blob/main/contracts/voucher/src/lib.rs#L1183)
  and [`pay_back`](https://github.com/Godgift-care/sci-healthcare-contracts/blob/main/contracts/voucher/src/lib.rs#L1286)
  send only to addresses stored on the voucher, never to the caller.
- The rule is pinned twice in tests: `settlement_is_permissionless` runs
  `settle` with no auth context at
  [line 1308](https://github.com/Godgift-care/sci-healthcare-contracts/blob/main/contracts/voucher/src/test.rs#L1308),
  and the authorisation matrix in
  [`authorisation_matrix_matches_every_public_function`](https://github.com/Godgift-care/sci-healthcare-contracts/blob/main/contracts/voucher/src/test.rs#L3060)
  records both rows as `Auth::Free("none (permissionless)")`, so adding a
  `require_auth()` to either call fails CI.

## Consequences

- A clinic never depends on the funder, the maintainers or an operator to be
  paid, and a funder can always recover an expired voucher without the clinic's
  cooperation.
- The caller pays the network fee and gets nothing for it. That makes
  settlement a public good, which is why the repository ships a keeper
  (`examples/keeper.ts`) that anyone can run. It also means no one can be
  censored by refusing to run it.
- Anyone can call, so anyone can cause the transfer to happen. They cannot
  choose a recipient, an amount, or the timing beyond "not yet": the stored
  voucher is the only source of those.
- Repeat calls are harmless. Once settled, another `settle` fails
  `InvalidStatus` and the whole invocation reverts; there is no double payout
  path.
- This is the opposite of the admin-only transitions (`resolve_dispute`,
  `resolve_split`, `set_*`), which do require a signature. A reviewer should be
  suspicious of any `require_auth` added to these two and of any admin call
  that starts moving escrow without a dispute.
- An integration that treats "settle" as privileged will unnecessarily wait for
  an operator. The docs say the opposite in
  [the contract reference](../developers/contracts.md).

## Alternatives rejected

- **Funder-triggered settlement.** The funder can withhold the call after an
  attestation it dislikes, which is exactly the griefing the dispute mechanism
  already bounds. It also makes the clinic's revenue depend on a stranger's
  uptime.
- **Provider-triggered settlement.** Better than funder-triggered, but the
  clinic still has to hold XLM and submit a transaction to be paid, and a
  clinic under a frozen account cannot. Permissionless with a keeper lets a
  third party pay the fee.
- **Admin or operator queue.** A single key that must run settlement is a
  liveness and censorship point, and it would need its own key-custody story.
  The protocol already has one trusted human role (dispute resolution) and
  should not grow a second one it does not need.
- **A cron job holding a funded key.** It is an operator in disguise: it can
  stop, be compromised, or run out of XLM. The keeper is that job made
  replaceable by anyone.
