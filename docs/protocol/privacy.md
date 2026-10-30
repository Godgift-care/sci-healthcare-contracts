---
title: Privacy
nav_order: 5
---

# Privacy

The single hardest rule in this project: **no protected health information reaches the ledger, ever.**

A public blockchain is permanent and worldwide. Anything written to it is written for good. Health data does not belong there, and most attempts to put it there — encrypted, hashed, or otherwise — eventually leak through metadata or key compromise.

## What is on chain

For each voucher:

| Field | Example | What it reveals |
| --- | --- | --- |
| `funder` | `GDUP…JG45` | A Stellar address that paid |
| `provider` | `GDOO…ZKBK` | Which clinic |
| `service_code` | `101` | A coarse category, e.g. "outpatient consult" |
| `amount` | `30000000` | $3.00 |
| `beneficiary_ref` | `72676a6f…094745` | An opaque 32 bytes |
| timestamps | | When it was funded, claimed, attested, settled |

## What is not, and never will be

- Names, dates of birth, phone numbers, addresses
- Diagnoses, symptoms, test results, prescriptions
- Any free-text field a clinician could type into

There is no field for these. Adding one would be rejected in review.

## Service codes stay coarse on purpose

`101` means "outpatient consult". It does not mean "HIV viral load test".

This is a real design tension. Finer codes would make the product more useful — better analytics, better pricing, better fraud detection. They would also turn the ledger into a permanent public record of what people are being treated for. A code specific enough to be clinically useful is specific enough to out someone.

So the codes stay blunt, and the protocol accepts being less useful for it.

## Beneficiary references

The reference is an HMAC-SHA256 of a patient identifier under a secret key:

```
beneficiary_ref = HMAC-SHA256(key, lowercase(trim(identifier)))
```

Computed in the patient's browser via Web Crypto. The identifier and the key never reach the API, are never logged, and never touch the ledger.

**Why HMAC and not a plain hash.** A plain `SHA256(phone_number)` is trivially reversible by brute force — there are only so many phone numbers, and an attacker can hash all of them. HMAC under a secret key is not searchable without that key.

**What this gives you.** Two vouchers for the same person produce the same reference, so a care history can be assembled. Someone holding only on-chain data cannot work out whose history it is, and cannot enumerate patients.

**What it does not give you.** This is pseudonymity, not anonymity. Anyone who already knows both the identifier and the key can confirm a match. Someone who watches a specific patient walk into a specific clinic at a specific time can correlate that with an on-chain event. Address reuse by a funder links their vouchers together.

Anyone claiming a public blockchain gives patients anonymity is either mistaken or selling something.

## The database

The indexer's database stores nothing that is not already public on chain. That is a deliberate constraint, and it means a breach of that database is not a health-data breach. It is a breach of a cache of public information.



## Service categories

The category catalogue is public on-chain data. Category labels must stay coarse (for example "Laboratory", never a specific test or condition), because a category on a voucher is visible to everyone.