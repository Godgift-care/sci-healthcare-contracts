---
title: Attesting delivery
nav_order: 8
---

# Attesting delivery

🇫🇷 [Lire ce guide en français](../fr/attesters.md) · 🇰🇪🇹🇿 [Soma mwongozo huu kwa Kiswahili](../sw/attesters.md)

An attester confirms that care actually happened. This is the most important role in the protocol and the one carrying the most responsibility.

## What you are doing

When you attest, you start a 72-hour clock. When it runs out, the clinic is paid automatically and irreversibly.

You are not approving a payment request. You are asserting a fact about the world: this patient received this service at this clinic.

## Who does this

Attesters are appointed by an administrator. In practice they are people already present in the care pathway and not employed by the clinic being paid — community health workers, supervising nurses at a different facility, NGO field staff, district health officers.

The point is independence. If the attester's income depends on the clinic being paid, the safeguard does not work.

## How

1. **Connect your wallet.** The Attest page shows every voucher waiting.
2. **Verify the care happened.** However your programme does it — being present, checking the register, speaking to the patient.
3. **Press Confirm care delivered.**

## What the contract enforces

- **You cannot attest for a clinic you are.** If the attester address matches the provider address, the call is rejected. Always, no exceptions.
- **You must be an authorised attester.** Revoked attesters are rejected.
- **The voucher must be `Claimed`.** You cannot attest before the clinic marks the patient seen, or twice.

## What it does not enforce

That you are telling the truth. This is the protocol's central trust assumption, documented plainly in the [trust model](../protocol/trust.md).

What it does do: your attestation is signed by your address and recorded on chain permanently. Every voucher you have ever confirmed is public and attributable. There is no anonymous attestation.

## Before you attest

Work through the [attester evidence checklist and code of conduct](attester-checklist.md) every time: what to confirm before you press **Confirm care delivered**, and when you must step aside.

## If you are unsure

Do not attest. An unattested voucher is not lost — it stays escrowed and refunds to the funder at expiry, or immediately if the clinic is no longer active. The failure mode of not attesting is a delay. The failure mode of wrongly attesting is a patient's money going to a clinic that did nothing, with no way to get it back after 72 hours.

If you believe a clinic is claiming for care it did not deliver, tell an administrator so it can be suspended.
