---
title: Trust model
nav_order: 4
---

# Trust model

This protocol is **trust-minimised, not trustless**. It is worth being exact about what that means, because overselling it would be the easiest thing to do and the most damaging.

## The unavoidable problem

Whether a nurse actually examined a patient is a fact about the physical world. No smart contract can observe it. Something off-chain has to assert it, and that assertion can be false.

Everything below is about narrowing the room to lie, not eliminating it.

## What the contract does guarantee

These hold regardless of who behaves badly:

1. **Funds can only reach the named clinic or return to the funder.** There is no third destination. Not the admin, not the protocol, not an attester.
2. **The clinic being paid cannot attest its own delivery.** The contract checks this explicitly and rejects it, even if that clinic address has been granted an attester role.
3. **A clinic cannot be paid without an attestation.** No path exists from `Funded` or `Claimed` straight to `Settled`.
4. **Money cannot be released early.** `settle` fails while the dispute window is open.
5. **An unattested voucher always returns to the funder.** Unclaimed, it is refundable at expiry; claimed but never attested, seven days after expiry. Nobody can prevent that — `refund` needs no authorisation.
6. **The full amount is refunded.** No fee is taken on care that did not happen.
7. **A settled voucher cannot be settled or refunded again.** Terminal states are terminal.
8. **Admin rights cannot be handed to a mistyped address.** Every contract transfers administration in two steps — `propose_admin`, then `accept_admin` signed by the new address.

## What you are trusting

Stated plainly, because these are real:

### Attesters

An attester can confirm care that never happened. If an attester and a clinic collude, they can drain vouchers for fictional visits.

What limits it: attesters are appointed by the admin and can be revoked; the provider cannot attest for itself; the funder has a 72-hour window to dispute before any money moves; and every attestation is permanently attributable on chain, so a dishonest attester leaves a complete record.

What would improve it: an attester quorum for high-value vouchers, so one compromised party is not enough. This is [an open issue](https://github.com/Godgift-care/sci-healthcare-contracts/issues), not something already built.

### The admin

The admin can activate and suspend clinics, appoint and revoke attesters, and resolve disputes.

What limits it: on a disputed voucher the admin's only choice is *which of the two parties* receives the escrowed funds. There is no call that lets the admin move money to a third address, mint vouchers, or take funds from a settled voucher. The admin cannot touch a voucher that is not in `Disputed`.

The admin key is a single point of failure today. Moving it to a multisig is a deployment decision, and mainnet should not launch without it.

### The registry

If a clinic is verified that should not have been, funders can send money to it. Verification is an off-chain process — checking a licence, visiting a site — and the registry only records its outcome.

## What the protocol deliberately does not claim

- It does not verify that care was *good*, only that someone attested it happened.
- It does not verify identity. The beneficiary reference is a pseudonym.
- It does not prevent a clinic from overcharging. It enforces a floor at the listed price, not a ceiling on what a clinic may list.
