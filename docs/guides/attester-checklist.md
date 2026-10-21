---
title: Attester checklist and code of conduct
nav_order: 9
---

# Attester evidence checklist and code of conduct

Your attestation releases a funder's money to a clinic, irreversibly, 72 hours after you sign. The contract can check *who* you are; it cannot check *what you saw*. This page is the standard for what you must see before you attest, and when you must not.

It is written against the rules the contract enforces (see [Attesting delivery](attesters.md) and the [trust model](../protocol/trust.md)). Where the contract already stops something, that is noted, so you know which rules rely on you alone.

## The evidence checklist

Confirm **every** item before you press **Confirm care delivered**. If one cannot be confirmed, do not attest.

| # | Check | How you know | Contract enforces it? |
| --- | --- | --- | --- |
| 1 | **The patient was present.** The person on the voucher was physically at the clinic. | You saw them, or a record made at the visit — the clinic register entry, a signed patient slip, the patient confirming when you call them. A clinic's word alone is not enough. | No — this is why attesters exist. |
| 2 | **The service matches.** What was delivered is the service the voucher names (its service code and label), not a cheaper one or a different one. | The register entry, the dispensing record, the test result slip. | No. The contract only knows the code the funder paid for. |
| 3 | **It happened at this clinic.** The care was delivered by the clinic named on the voucher, not referred elsewhere. | The register is that clinic's own. | No. |
| 4 | **It happened once.** This visit has not already been used for another voucher. | Each register entry matches one voucher. | Partly: the contract refuses to attest the same voucher twice, but cannot tell two vouchers apart for one visit. |
| 5 | **The voucher is `Claimed`.** The clinic has marked the patient seen. | The Attest page lists only claimed vouchers. | Yes — attesting anything else is rejected. |
| 6 | **You have no conflict of interest** (see below). | You. | Partly: the clinic's own address is always rejected. Nothing else is. |

Keep a note of the evidence you relied on for each voucher — date, what you checked, who you spoke to. Every attestation is public and attributable to your address forever; if it is ever disputed, that note is your account of it. Do not record anything that identifies the patient beyond what your programme already holds; the voucher itself carries only an opaque reference.

## Code of conduct

**Never attest for your own clinic.** The contract rejects an attestation from the clinic address being paid, but it cannot see that you work there, own part of it, or are paid by it. If the clinic's income is your income, you must not attest its vouchers.

**Recuse when you are related.** Step aside, and tell the administrator so another attester can take the voucher, when:

- the patient is a relative or a member of your household;
- someone at the clinic is a relative, a business partner, or pays you in any way;
- you, or someone close to you, funded the voucher;
- you have any other reason someone could reasonably doubt your independence.

**Never attest on someone else's say-so.** "The clinic told me" and "the patient's relative said so" are not evidence. If you could not confirm the checklist yourself, the answer is to wait.

**Never accept anything for an attestation.** No payment, gift or favour from a clinic, a patient or a funder, whether before or after. Report any offer to an administrator.

**Never share your key.** Your attester key is your signature. If someone else holds it, they can release money in your name — the daily attestation cap only limits how much before the administrator removes it. If you think your key is exposed, tell an administrator immediately so it can be removed from the registry.

**Do not rush.** An unattested voucher is never lost: it stays in escrow and returns to the funder at expiry. A wrong attestation sends a patient's money to a clinic with no way back after 72 hours. When in doubt, do not attest.

**Report what you see.** If a clinic is claiming for care that did not happen, or pressing you to attest, tell an administrator so the clinic can be suspended. Reporting is part of the role, not a breach of trust with the clinic.

## What happens if these are broken

An administrator can remove an attester from the registry at any time (`remove_attester`), after which every attestation that attester tries is rejected. Attestations already made stay on chain, signed by your address, and are the record a dispute is resolved against.
