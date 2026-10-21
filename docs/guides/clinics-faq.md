---
title: Clinic FAQ
nav_order: 9
---

# Clinic FAQ

Common questions about running a clinic on the protocol, each answered with a link to the source. Numeric claims (fees, windows) match the contract constants in [the contract reference](developers/contracts.md) and `scripts/deploy.sh`.

## When am I paid?

After the dispute window closes. Once an attester confirms delivery, the window runs for 72 hours in the production configuration. When it closes, anyone — including you — can call `settle`, and you receive the voucher amount less the 1% protocol fee. A $3.00 outpatient consult pays you $2.97; settlement is automatic and irreversible, and no operator can withhold it.

*Sources: [How a voucher works — Attested and Settled](protocol/lifecycle.md#attested), [Economics — the fee](protocol/economics.md#the-fee), [Running a clinic — getting paid](guides/clinics.md#getting-paid)*

## What if the patient never comes?

If the voucher is never claimed, it expires and you cannot be paid on it — the funder can refund the full amount with no fee. If the clinic is suspended before the voucher is claimed, the funder can refund immediately rather than waiting for expiry. If you claimed it but no attester ever confirms the visit, the money still goes back to the funder seven days after expiry (the claim grace period); you are not entitled to payment without an attestation.

*Sources: [How a voucher works — Refunded](protocol/lifecycle.md#refunded), [Funding care — if something goes wrong](guides/funders.md#if-something-goes-wrong)*

## How do disputes work?

The funder can press **Dispute** while the voucher is `Claimed` or `Attested`, but only before the dispute window closes — 72 hours after attestation in production. Funds freeze in escrow and an administrator resolves it in favour of either the funder or the clinic. The admin can never send the money to a third party, and once the window closes the payment is automatic and cannot be clawed back.

*Sources: [How a voucher works — Disputed](protocol/lifecycle.md#disputed), [Trust model — the admin](protocol/trust.md#the-admin)*

## What fees apply?

The protocol fee is 1% (100 basis points), taken once at settlement. It is capped at 1000 bps (10%) at initialisation, so a deployment can never be reconfigured to charge more. Refunds are free and funding is free; you also pay the Stellar network fee only for transactions you submit (claim, and settle if you trigger it yourself), which are a fraction of a cent in XLM.

*Sources: [Economics — the fee](protocol/economics.md#the-fee), [Economics — who pays what](protocol/economics.md#who-pays-what), [Contract reference — fee cap](developers/contracts.md#initialize)*

## Can I claim after the voucher expires?

No. A voucher cannot be claimed after its `expires_at`, and a clinic that was suspended after the voucher was funded cannot claim it at all. If the patient turns up after expiry, the funder refunds the money instead. If the clinic is suspended before the voucher is claimed, the funder can refund immediately.

*Sources: [How a voucher works — Claimed](protocol/lifecycle.md#claimed), [Contract reference — claim](developers/contracts.md#claim)*

## Can I attest my own voucher?

No. The contract rejects `attest` when the caller is the clinic being paid, even if that address holds an attester role. You must use an independent attester — a community health worker, a supervising nurse at a different facility, or similar — appointed by the admin.

*Sources: [Trust model — what the contract guarantees](protocol/trust.md#what-the-contract-does-guarantee), [Attesting delivery — what the contract enforces](guides/attesters.md#what-the-contract-enforces)*

## What if my clinic is suspended?

A suspended clinic cannot receive new vouchers or change prices, but vouchers already funded still settle normally. You can keep delivering on existing vouchers and be paid once they are attested and the dispute window closes.

*Sources: [Running a clinic — suspension](guides/clinics.md#suspension)*

## Who pays the Stellar network fee?

Whoever submits the transaction pays it: you pay for `claim`, the attester pays for `attest`, the funder pays for `create_voucher`, and whoever triggers `settle` or `refund` pays for those. Because `settle` and `refund` are permissionless, an operator can run a keeper that settles eligible vouchers for everyone and absorbs those fees — but the protocol does not depend on one existing.

*Sources: [Economics — network costs](protocol/economics.md#network-costs), [Economics — who pays what](protocol/economics.md#who-pays-what)*

## Can someone else release my payment?

Yes. `settle` needs no authorisation, so once the dispute window has closed any caller can trigger settlement of your attested voucher — you never have to chase the funder or an operator to be paid.

*Sources: [Economics — who pays what](protocol/economics.md#who-pays-what), [Contract reference — settle](developers/contracts.md#settle)*

## What currency am I paid in, and how do I get local cash?

Settlement is in USDC. Converting to naira, shillings or cash goes through a Stellar anchor — Yellowcard, Cowrie, HoneyCoin or MoneyGram — which is a separate, regulated step with its own identity checks.

*Sources: [Running a clinic — getting local currency](guides/clinics.md#getting-local-currency)*

## Is there a maximum fee the protocol could charge?

Yes. The fee is capped at 1000 basis points (10%) and this ceiling is enforced at voucher contract initialisation, before any money moves. Production is 100 bps (1%).

*Sources: [Economics — the fee](protocol/economics.md#the-fee), [Contract reference — initialize](developers/contracts.md#initialize)*

## How long do I have to dispute a voucher as a funder?

Not a clinic question, but clinics get asked this: the funder can dispute while the voucher is `Claimed`, or while it is `Attested` and before the deadline — 72 hours after attestation in production. Once the window closes, settlement is automatic.

*Sources: [How a voucher works — Attested](protocol/lifecycle.md#attested), [Funding care — if something goes wrong](guides/funders.md#if-something-goes-wrong)*

## Does the protocol guarantee the care was good?

No. It guarantees that an independent attester said the care happened, that you had a window to object, and that the money can only go to your clinic or back to the funder. Quality of care is between you, the patient and your regulators.

*Sources: [Trust model — what the protocol deliberately does not claim](protocol/trust.md#what-the-protocol-deliberately-does-not-claim)*
