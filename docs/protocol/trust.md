---
title: Trust model
nav_order: 4
language: en
translations:
  fr: /fr/protocol/trust.html
  sw: /sw/protocol/trust.html
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
5. **An unattested voucher always returns to the funder.** Unclaimed, it is refundable at expiry — or immediately if the clinic is no longer active. Claimed but never attested, seven days after expiry. Nobody can prevent that — `refund` needs no authorisation.
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

Every status change in the registry records **why**, as a `reason_code` stored on the provider and emitted in `provider_status_changed`. A suspended clinic reads its code and knows what to fix; a funder reading the registry's history can tell a fraud suspension from expired paperwork. The registry is deliberately dumb about the values — it stores and publishes the code, it does not interpret it — so apps must map the codes themselves:

#### Reason codes

| Code | Meaning |
| --- | --- |
| 0 | No status change yet (set at registration; `set_provider_status` refuses it with `MissingReasonCode`) |
| 1 | Verified |
| 2 | Documents expired |
| 3 | Complaint under review |
| 4 | Fraud |
| 5 | Closed (given when a clinic retires itself) |

Codes are **added, never renumbered or reused**. A retired code keeps its meaning for every deployment and every historical event, so an audit of last year's suspensions still reads correctly. New meanings get new codes appended here in the same pull request that first emits them.

The other thing the registry records about status is what it refuses: `set_provider_status` only accepts `Pending → Active`, `Pending → Suspended`, `Active → Suspended` and `Suspended → Active` (see the [status lifecycle](../developers/contracts.md#status-lifecycle)), so the event history contains no no-op changes and no backwards moves nobody can explain. A clinic that closes retires itself; nothing — not even the admin — can bring a retired clinic back, so no reason code is ever an exit from `Retired`.

Reinstating a suspended clinic (`Suspended → Active`) re-opens it to funding, so it is held to the same rule as every other change: only the stored admin, with its own signature, and always with a non-zero reason code. The clinic cannot reinstate itself, and neither can an attester, another clinic, an admin nominee who has not accepted, or an admin who has handed over. Each of those has a test.

### The settlement token's issuer

USDC is a regulated stablecoin, and its issuer can **claw back** funds from any balance and **freeze** (deauthorize) one. Escrowed vouchers sit in the voucher contract's own token balance, so the issuer can reach them; nothing in the contract can prevent that. What the contract does is behave predictably when it happens:

- **Clawback.** Before paying anything out, `settle`, `refund`, `resolve_dispute`, `resolve_split` and `reduce_amount` check that the contract still holds the voucher's amount. If it does not, they fail with `EscrowShortfall` (#23) instead of a panic inside the token. Escrow is one balance shared by every open voucher, so a clawback is borne by whichever vouchers draw on it last: earlier ones settle or refund in full, and the rest fail with `EscrowShortfall` until the balance is restored.
- **Freeze.** If the issuer has frozen the contract, the clinic or the funder, the token refuses the transfer and the call fails with `TokenTransferFailed` (#24).
- **Fee-charging or hook tokens.** The contract assumes the settlement token moves exactly the amount requested, which USDC's Stellar asset contract does. A token that skims a transfer fee, debits the sender extra or runs a hook that changes balances would break the escrow invariant without anyone noticing. So the contract measures balances around every transfer. Funding checks that the escrow rose by exactly the voucher amount. Every payout checks that the escrow fell by exactly the amount sent and the recipient rose by exactly that amount. Any difference fails with `TokenAmountMismatch` (#34). The `nonstandard_token` tests in `contracts/voucher/src/test/nonstandard_token.rs` run the lifecycle against a mock token in each of these modes.
- **Nothing is half-paid.** Any of these errors reverts the whole call: the voucher keeps its status, no receipt is minted and no fee is taken. A voucher held up by a freeze settles or refunds normally once the freeze is lifted.

These are covered by the `clawback_*`, `partial_clawback_*` and `frozen_*` tests in `contracts/voucher/src/test.rs`, which run against a Stellar asset contract with clawback and revocation enabled.

## What the protocol deliberately does not claim

- It does not verify that care was *good*, only that someone attested it happened.
- It does not verify identity. The beneficiary reference is a pseudonym.
- It does not prevent a clinic from overcharging. It enforces a floor at the listed price, not a ceiling on what a clinic may list.
