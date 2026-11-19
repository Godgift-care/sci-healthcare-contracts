---
title: Worked examples — attester dispute decisions
nav_order: 10
---

# Worked examples — attester dispute decisions

These five cases illustrate how to apply the [attester evidence checklist](../guides/attester-checklist.md) and the [trust model](../protocol/trust.md) when deciding whether to attest. Each case names the evidence that decided it.

> **Rule of thumb**: If you cannot confirm every item on the checklist yourself, do not attest. An unattested voucher is never lost — it stays in escrow and returns to the funder. A wrong attestation sends a patient's money to a clinic with no way back after 72 hours.

---

## Case 1 — Clear: routine outpatient visit

**Situation**  
A funder escrows a voucher for "Outpatient consult" (code 101) at Ikeja General Clinic. The patient arrives, the clinic marks the voucher `Claimed`. You are the assigned attester, a community health worker not employed by the clinic.

**Evidence you confirm**  
| Checklist item | How you confirmed |
| --- | --- |
| 1. Patient was present | You were in the waiting room; saw the patient check in and enter the consultation room. |
| 2. Service matches | The clinic register entry for that time slot reads "Outpatient consult — code 101". |
| 3. Happened at this clinic | The register is Ikeja General's own; you are physically at that facility. |
| 4. Happened once | This register entry has not been used for any other voucher. |
| 5. Voucher is `Claimed` | The Attest page shows status `Claimed`. |
| 6. No conflict of interest | You are not employed by, related to, or paid by the clinic. |

**Decision**  
**Attest.** All six checklist items are confirmed from direct observation or the clinic's own register (which you trust because you saw the patient enter).

**Why**  
This is the standard case the protocol is built for. Your attestation starts the 72-hour dispute window; the funder can object if they have evidence to the contrary.

---

## Case 2 — Borderline: patient seen but service code mismatch

**Situation**  
Voucher is for "Lab panel — basic" (code 102, $5.00). The clinic claims the voucher. You check the register: the entry says "Rapid malaria test — code 105" (price $3.00). The clinic says "it's basically the same thing".

**Evidence you confirm**  
| Checklist item | How you confirmed |
| --- | --- |
| 1. Patient was present | You saw the patient; register confirms attendance. |
| 2. Service matches | **NO.** Register shows code 105, voucher is for code 102. Different service, different price. |
| 3. Happened at this clinic | Yes, register is this clinic's. |
| 4. Happened once | Register entry used once. |
| 5. Voucher is `Claimed` | Yes. |
| 6. No conflict of interest | Yes. |

**Decision**  
**Do not attest.** Checklist item 2 fails — the service delivered does not match the voucher.

**Why**  
The protocol enforces a price floor at the listed price for the service code on the voucher. Attesting a mismatched service would pay the clinic for a more expensive service than was delivered. The clinic should have listed the correct code, or the funder should have funded the correct code. If you attest anyway, you are confirming something you know is false.

**What to do**  
Tell the clinic the voucher code is wrong. They can ask the funder to `redirect` the voucher (if still `Funded`) or the funder can dispute. Do not attest until the code matches what was delivered.

---

## Case 3 — Fraudulent: clinic claims for a patient who never came

**Situation**  
Voucher for "Outpatient consult" at a clinic you cover. The voucher is `Claimed`. You call the patient (number on file) — they say they never visited. You check the clinic register for that day — no entry for that patient. The clinic insists "the patient came, the register must have been lost".

**Evidence you confirm**  
| Checklist item | How you confirmed |
| --- | --- |
| 1. Patient was present | **NO.** Patient denies it; register has no entry. Clinic's word alone is not enough (checklist explicitly says so). |
| 2. Service matches | Cannot confirm — no register entry. |
| 3. Happened at this clinic | Cannot confirm. |
| 4. Happened once | Cannot confirm. |
| 5. Voucher is `Claimed` | Yes. |
| 6. No conflict of interest | Yes. |

**Decision**  
**Do not attest.** Checklist item 1 fails — you cannot confirm the patient was present.

**Why**  
The checklist is explicit: "A clinic's word alone is not enough." Without a register entry or your own observation, you have no evidence. Attesting would send money for care that did not happen.

**What to do**  
Do not attest. Report the clinic to the administrator (suspension reason code 4 = fraud). The funder can dispute while the window is open, or wait for the voucher to become refundable after the claim grace period (7 days post-expiry).

---

## Case 4 — Clinic error: double-claiming one visit

**Situation**  
A patient visits for "Outpatient consult" (code 101). The clinic claims voucher #42 for that visit. Later the same day, the clinic claims voucher #43 (also code 101, same patient) — a different funder, same clinic. You are the attester for both.

**Evidence you confirm**  
| Checklist item | Voucher #42 | Voucher #43 |
| --- | --- | --- |
| 1. Patient was present | Yes — you saw them. | Yes — same patient. |
| 2. Service matches | Yes — code 101. | Yes — code 101. |
| 3. Happened at this clinic | Yes. | Yes. |
| 4. Happened once | **YES** for #42. | **NO** — this visit already used for #42. |
| 5. Voucher is `Claimed` | Yes. | Yes. |
| 6. No conflict of interest | Yes. | Yes. |

**Decision**  
**Attest voucher #42. Do not attest voucher #43.**

**Why**  
Checklist item 4 ("It happened once") fails for the second voucher. The contract prevents attesting the *same voucher* twice, but it cannot tell two *different vouchers* apart for one visit. That is your job. The register has one entry for that patient visit — it can only support one voucher.

**What to do**  
Attest the first voucher. For the second, tell the clinic they cannot double-claim. The funder of #43 can dispute, or the voucher will refund after the claim grace period. Report the pattern to the admin if it repeats.

---

## Case 5 — Funder error: wrong clinic on the voucher

**Situation**  
A funder escrows a voucher for "Outpatient consult" at Clinic A. The patient actually went to Clinic B (also Active, same service code). Clinic A claims the voucher. You are an attester for Clinic A.

**Evidence you confirm**  
| Checklist item | How you confirmed |
| --- | --- |
| 1. Patient was present | **NO.** The patient was at Clinic B, not Clinic A. Clinic A's register has no entry. |
| 2. Service matches | Cannot confirm — no visit at Clinic A. |
| 3. Happened at this clinic | **NO.** Care happened at Clinic B. |
| 4. Happened once | N/A. |
| 5. Voucher is `Claimed` | Yes. |
| 6. No conflict of interest | Yes. |

**Decision**  
**Do not attest.** Checklist items 1 and 3 fail — the patient was not at this clinic.

**Why**  
The voucher names a specific provider. The contract checks that the attester is not the provider being paid, but it cannot check whether the care happened at *that* provider. If you attest, you confirm the patient was at Clinic A when they were not.

**What to do**  
Do not attest. The funder should use `redirect` (while `Funded`) to move the voucher to Clinic B, or dispute if already `Claimed`. If the funder does nothing, the voucher will refund at expiry (or immediately if Clinic A is suspended).

---

## Summary table

| Case | Attest? | Failing checklist item(s) | Key evidence |
| --- | --- | --- | --- |
| 1. Clear | **Yes** | None | Direct observation + clinic register |
| 2. Borderline (code mismatch) | **No** | #2 Service matches | Register shows different code |
| 3. Fraudulent (ghost patient) | **No** | #1 Patient present | Patient denies; no register entry |
| 4. Clinic error (double claim) | **No** for 2nd | #4 Happened once | One register entry, two vouchers |
| 5. Funder error (wrong clinic) | **No** | #1, #3 Patient present, at this clinic | Patient was at a different clinic |

---

## What to do when unsure

1. **Do not attest.** The failure mode of not attesting is a delay (money stays in escrow, refunds later). The failure mode of wrongly attesting is irreversible loss.
2. **Ask for more evidence.** Speak to the patient, check the register again, ask the clinic for the dispensing record or test result slip.
3. **Escalate.** If a clinic pressures you, or you suspect fraud, tell an administrator so the clinic can be suspended (reason code 3 = complaint under review, or 4 = fraud).
4. **Document.** Keep a note of what you checked, when, and what you found. Your attestation is permanently attributable to your address on chain.

---

## References

- [Attester evidence checklist and code of conduct](../guides/attester-checklist.md)
- [Attesting delivery](../guides/attesters.md)
- [Trust model — what the contract guarantees](../protocol/trust.md#what-the-contract-does-guarantee)
- [Trust model — attesters](../protocol/trust.md#attesters)