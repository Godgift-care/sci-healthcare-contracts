---
title: "0003 — Beneficiary references are HMACs, not hashes"
parent: Architecture decision records
nav_order: 3
---

# 0003 — Beneficiary references are HMACs, not hashes

- **Status:** Accepted
- **Date:** 2026-11-20
- **Decision area:** contracts, privacy

## Context

A voucher needs a 32-byte `beneficiary_ref` so that two episodes for the same
person can be linked — a receipt count, a care history — without putting a
name, a phone number or a national ID on a public ledger.

A plain hash looks like the obvious answer: `SHA256(phone_number)` is short,
deterministic, and hides the input. It does not hide the input for long. The
space of Nigerian phone numbers, or of clinic card numbers, is small enough to
enumerate; an attacker hashes every candidate and matches the ledger. The
"hashed" field becomes a lookup table, and every voucher for a person becomes
attributable to a number. That is the specific failure this field exists to
avoid.

The contract cannot fix this after the fact. It receives 32 bytes and has no
way to know how they were produced: an HMAC, a plain hash and random bytes are
indistinguishable to it. The choice has to be made and documented above the
contract.

## Decision

`beneficiary_ref` is an HMAC-SHA256 of a normalised patient identifier under a
per-patient secret key, derived on the user's device before the transaction is
signed:

```text
beneficiary_ref = HMAC-SHA256(key, lowercase(trim(identifier)))
```

- The reference implementation is
  [`docs/developers/integration.md` — `deriveBeneficiaryRef`](../developers/integration.md#1-derive-the-beneficiary-reference)
  (`createHmac('sha256', key)` in Node, the equivalent Web Crypto import in the
  browser), with `generateBeneficiaryKey` producing a random key per patient.
- The contract stores the 32 bytes opaquely and never tries to interpret them:
  [`contracts/voucher/src/types.rs` — `Voucher::beneficiary_ref`](https://github.com/Godgift-care/sci-healthcare-contracts/blob/main/contracts/voucher/src/types.rs#L41)
  and the positional storage form in
  [`contracts/voucher/src/storage.rs` — `StoredVoucher`](https://github.com/Godgift-care/sci-healthcare-contracts/blob/main/contracts/voucher/src/storage.rs#L25).
- It is written by
  [`contracts/voucher/src/lib.rs` — `VoucherEscrow::fund`](https://github.com/Godgift-care/sci-healthcare-contracts/blob/main/contracts/voucher/src/lib.rs#L902),
  indexed as a topic by
  [`contracts/voucher/src/events.rs` — `VoucherCreated`](https://github.com/Godgift-care/sci-healthcare-contracts/blob/main/contracts/voucher/src/events.rs#L24),
  and read back per person by
  [`contracts/receipt/src/lib.rs` — `ReceiptBook::count_for`](https://github.com/Godgift-care/sci-healthcare-contracts/blob/main/contracts/receipt/src/lib.rs#L361).
- The privacy rationale, including what this does *not* protect, is
  [Privacy — beneficiary references](../protocol/privacy.md#beneficiary-references).

## Consequences

- On-chain data alone cannot be searched for a given phone number without the
  key, and the number of candidate identifiers no longer bounds the attack.
- Two vouchers for the same person under the same key produce the same
  reference, which is what makes a care history possible. A different key gives
  an unlinkable reference, deliberately.
- This is pseudonymity, not anonymity. Anyone holding both the identifier and
  the key can confirm a match, and anyone who watches a person enter a clinic
  can correlate the on-chain event by time and provider.
- Losing the key loses the ability to look up that person's history. This is a
  real product cost and the reason clients must prompt the user to keep it.
- The guarantee is **not enforced by the contract**. A wallet can still send
  `SHA256(phone_number)` and the contract will accept it; nothing in `fund`
  inspects the bytes. The rule is a protocol rule that integrators and review
  must hold, which is exactly why it is written down here.
- Two code comments still call the field an "opaque salted hash"
  (`contracts/voucher/src/lib.rs` module docs and
  `contracts/voucher/src/types.rs`), and
  [the audit scope](../developers/audit-scope.md) says "salted SHA-256 hash".
  That shorthand is stale: the derivation above, [Privacy](../protocol/privacy.md#beneficiary-references)
  and the integration guide are the authority. Correcting those comments is
  a docs-only follow-up so this ADR does not hide the disagreement.
- Demo and seed scripts use a plain `sha256` of a fixed string as a stand-in
  (`examples/lifecycle.ts`, `scripts/seed.sh`). That is not the protocol
  reference derivation and must not be copied into a real client.
- This field is in the topic list of `voucher_created`, so an indexer stores it.
  It is public by design, but it must never be joined with off-chain identity
  data in a way that publishes the join.

## Alternatives rejected

- **`SHA256(identifier)`, or `SHA256(salt || identifier)` with a public salt.**
  Reversible by enumerating the identifier space; a public salt does not help
  because the salt is published too. This is the mistake the ADR exists to
  prevent.
- **Encrypt the identifier on chain.** It puts ciphertext on a permanent public
  ledger, creates a decryption key that must be available to render the field
  useful, and key compromise retroactively exposes every historical row. It
  also makes the contract or an indexer a decryption service.
- **A sequential patient id assigned by the protocol.** It links every episode
  trivially and is enumerable: anyone can walk ids and count a clinic's
  patients by service. It also needs an authority to assign ids, which is
  another admin role.
- **Store nothing and rely on the indexer.** Then the contract has no way to
  count a person's settled episodes, the receipt book's `count_for` cannot
  exist, and the same data ends up in a private database with weaker
  guarantees than a public commitment.
- **Derive the reference on the server.** The identifier then reaches a server
  and its logs, which the privacy rule forbids. Derivation stays on the
  device for the same reason the key does.
