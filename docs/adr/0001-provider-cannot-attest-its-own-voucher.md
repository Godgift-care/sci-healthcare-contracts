---
title: "0001 — A provider cannot attest its own voucher"
parent: Architecture decision records
nav_order: 1
---

# 0001 — A provider cannot attest its own voucher

- **Status:** Accepted
- **Date:** 2026-11-20
- **Decision area:** contracts

## Context

Money leaves escrow when someone confirms that care was delivered, and that
confirmation is an off-chain fact the chain cannot observe. The protocol has a
registry of addresses allowed to attest (`registry::add_attester`), and a clinic
can legitimately be one of them: a hospital group may employ a compliance
officer who attests for several of its sites.

The dangerous case is narrower. If the address that receives the voucher can
also attest that same voucher, then it can move the funder's money to itself
with two calls and no independent check. The funder's only remaining protection
would be to notice within the dispute window and contest it — a window the
provider controls the start of, because `attest` is what opens it.

A clinic is also the party with the strongest incentive to claim care it did
not deliver, so this is exactly the role that must not be able to sign off on
itself. "It holds an attester role" is not evidence of independence.

## Decision

`attest` refuses any attester that equals the voucher's provider, even when the
registry says that address is an attester:

- [`contracts/voucher/src/lib.rs` — `VoucherEscrow::attest`](https://github.com/Godgift-care/sci-healthcare-contracts/blob/main/contracts/voucher/src/lib.rs#L455)
  checks `registry.is_attester(&attester)` first and then returns
  `VoucherError::NotAuthorized` when `attester == voucher.provider`.
- [`contracts/voucher/src/lib.rs` — `VoucherEscrow::claim`](https://github.com/Godgift-care/sci-healthcare-contracts/blob/main/contracts/voucher/src/lib.rs#L395)
  is signed by the provider (the `require_auth()` sits in the shared
  `claim_voucher`), so the same address that marks the patient as presented
  cannot confirm delivery.
- [`contracts/registry/src/lib.rs` — `Registry::is_attester`](https://github.com/Godgift-care/sci-healthcare-contracts/blob/main/contracts/registry/src/lib.rs#L629)
  and [`Registry::add_attester`](https://github.com/Godgift-care/sci-healthcare-contracts/blob/main/contracts/registry/src/lib.rs#L473)
  are the role check and the admin-only grant the voucher relies on.
- Covered by
  [`provider_cannot_attest_its_own_payment`](https://github.com/Godgift-care/sci-healthcare-contracts/blob/main/contracts/voucher/src/test.rs#L1387),
  which gives the provider an attester role first, so the test proves the
  identity check and not merely a missing role.

## Consequences

- A clinic that wants to be paid must have its delivery confirmed by a
  different address. The protocol's trust is concentrated in the attester role:
  a colluding attester can still confirm care that did not happen. This ADR
  narrows the room to cheat; it does not remove it.
- The registry admin must keep attesters independent enough to be meaningful.
  Adding a clinic's own operations wallet as an attester is legal in the
  contract but useless for every voucher that clinic is paid for, and it is the
  kind of thing an audit should question.
- A provider that holds an attester role can still attest other clinics'
  vouchers. The check is per voucher, not a denial of the role.
- The error is `NotAuthorized` rather than a dedicated variant, so a caller
  cannot distinguish "not an attester" from "the provider itself". That is
  deliberate: both mean the address may not attest this voucher.
- Integration tests and demos must use two accounts on the clinic side. The
  seeded demo does, and
  [`scripts/seed.sh`](https://github.com/Godgift-care/sci-healthcare-contracts/blob/main/scripts/seed.sh)
  creates separate clinic and attester identities.

## Alternatives rejected

- **Let the provider attest, but give the funder a longer dispute window.** It
  converts the protocol into a charge-then-refund escrow. The clinic is paid
  unless the funder is watching, and the funder may be in another country,
  asleep, or deceased. Time-based protection is weaker than role separation and
  costs the honest clinic a longer wait.
- **Let the funder confirm delivery instead.** The funder is often not present
  at the appointment — sending money from abroad is the primary use case — so
  it would either confirm blindly or block payment it cannot verify. It also
  makes the payer the arbiter of their own refund.
- **Require a patient signature or proof of presence alone.** Presence is not
  delivery, and key custody for patients is the hardest problem in the product.
  A presence commitment already exists as an optional extra
  (`VoucherOptions::presence_commitment`), but it answers "did someone turn
  up", not "was care given".
- **Trust an off-chain verification service and remove the attester role.** It
  moves the same decision to one server, makes the answer unverifiable from
  chain data, and turns a role the admin can audit into an off-ledger
  dependency.
