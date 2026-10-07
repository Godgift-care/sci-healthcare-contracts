---
title: Running a clinic
nav_order: 7
---

# Running a clinic

You are a clinic, pharmacy or lab that wants to be paid through the protocol.

## Getting verified

1. **Connect the clinic's wallet** on the Clinic desk page. This address will receive every payment — use one you control and can keep secure.
2. **Register.** Enter the clinic name and two-letter country code. This puts you in the registry as `Pending`.
3. **Get verified.** An administrator reviews the clinic — licence, location, contact — and moves you to `Active`. Until then you cannot list services or receive vouchers.

Verification is a human process. Registering does not verify you.

## Listing services

Once active, list what you bill for. Each service needs:

- **A code.** A number identifying the category. Use a consistent scheme.
- **A label.** What patients will see, e.g. "Outpatient consult".
- **A price** in USDC.

> Keep labels and codes **coarse**. "Outpatient consult", not "HIV counselling session". These are published on a public blockchain permanently, and a code specific enough to be clinically useful is specific enough to expose a patient.

Prices are a floor. A funder may pay more, never less. Update a price at any time — vouchers already funded keep the amount agreed when they were created.

## Getting paid

1. **The patient arrives.** Find their voucher on the Clinic desk and press **Mark patient seen**. This is `claim`.
2. **Deliver the care.**
3. **An attester confirms it.** Not you — the contract rejects an attestation from the clinic being paid. This is the protocol's main safeguard and it is not something you can work around.
4. **The dispute window runs.** 72 hours in which the funder can object.
5. **You are paid.** After the window, anyone can trigger settlement, including you. Press **Release payment**.

You receive the voucher amount less the 1% protocol fee. A $3.00 voucher pays you $2.97.

## Things worth knowing

**You do not depend on anyone to be paid.** Settlement is permissionless. Once the window closes, the funds are yours to claim and no operator can withhold them.

**Claim promptly.** A voucher cannot be claimed after it expires — the funder gets it back.

**Suspension.** An administrator can suspend a clinic. A suspended clinic cannot receive new vouchers or change prices. Vouchers already funded still settle normally.

**Getting local currency.** Settlement is in USDC. Converting to naira, shillings or cash goes through a Stellar anchor — Yellowcard, Cowrie, HoneyCoin, MoneyGram. That is a separate, regulated step with its own identity checks. The in-app handoff is [an open issue](https://github.com/Godgift-care/sci-healthcare-frontend/issues), not yet built.
