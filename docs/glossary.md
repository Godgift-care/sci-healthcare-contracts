---
title: Glossary of protocol terms
nav_order: 1
---

# Glossary of protocol terms

Shared vocabulary for the SCI Healthcare protocol. Each entry is one or two sentences and links to where the term is defined or used, so issues and PRs start from the same understanding.

## A

### Attester
A person or wallet authorised by the admin to confirm that care was delivered. An attester calls `attest` on a `Claimed` voucher, which opens the dispute window; the clinic being paid can never attest its own voucher. Attesters are appointed and revoked through the registry.

Used in: [How a voucher works](protocol/lifecycle.md#attested), [Trust model — attesters](protocol/trust.md#attesters), [Attesting delivery](guides/attesters.md).

### Attestation
The on-chain statement that a specific patient received a specific service at a specific clinic. It is signed by the attester's address and recorded permanently; it is an assertion about the physical world, not a guarantee the care was good.

Used in: [How a voucher works](protocol/lifecycle.md#attested), [Trust model](protocol/trust.md).

## B

### Basis points (bps)
A unit equal to one hundredth of a percent (0.01%). The protocol fee is expressed in basis points: 100 bps is 1%. Fees use integer math: `fee = amount * fee_bps / 10_000`.

Used in: [Economics — the fee](protocol/economics.md#the-fee).

### Beneficiary reference (`beneficiary_ref`)
An opaque 32-byte value that links care episodes for one person without revealing their identity. It is the HMAC-SHA256 of a patient identifier under a secret key, computed in the patient's browser; neither the identifier nor the key ever reaches the chain.

Used in: [How a voucher works — worked example](protocol/lifecycle.md#a-worked-example), [Privacy — beneficiary references](protocol/privacy.md#beneficiary-references).

## C

### Claim
The clinic's call to `claim`, saying "this patient is in front of me". A voucher can only be claimed while `Funded`, before it expires, and only while the clinic is still `Active`. A claim is not a payment.

Used in: [How a voucher works — Claimed](protocol/lifecycle.md#claimed).

### Claim grace (period)
Seven days after a voucher's expiry during which a `Claimed` but never-attested voucher can still be refunded to the funder. It exists so a claim alone can never hold the funder's money hostage. Defined in the contract as `CLAIM_GRACE_SECS` (7 × 24 × 60 × 60 seconds).

Used in: [How a voucher works — Claimed](protocol/lifecycle.md#claimed), [Contract reference — refund](developers/contracts.md#refund).

### Clinic (provider)
A verified health business — clinic, pharmacy or lab — that lists services and receives settlement. In the registry it is a `Provider` with status `Pending`, `Active` or `Suspended`; only `Active` clinics can receive vouchers or claim them.

Used in: [Running a clinic](guides/clinics.md), [Contract reference — registry](developers/contracts.md#registry).

## D

### Dispute
The funder's objection to a claim, made with `dispute` while the voucher is `Claimed` or `Attested` and before the dispute window closes. Funds stay in escrow until the admin resolves it. A dispute carries a `reason_code` that the contract stores without interpreting.

Used in: [How a voucher works — Disputed](protocol/lifecycle.md#disputed), [Trust model — the admin](protocol/trust.md#the-admin).

### Dispute window
The period, starting when an attester confirms delivery, during which the funder can dispute. It is 72 hours in the production configuration and is set as `dispute_window` at voucher contract initialisation. `settle` fails while it is open.

Used in: [How a voucher works — Attested](protocol/lifecycle.md#attested), [How a voucher works — worked example](protocol/lifecycle.md#a-worked-example).

## E

### Escrow
The USDC held by the voucher contract between funding and settlement or refund. Money can leave escrow only to the named clinic or back to the funder — never to the admin, the protocol, or an attester.

Used in: [How a voucher works — Funded](protocol/lifecycle.md#funded), [Trust model](protocol/trust.md).

### Expiry (`expires_at`)
The deadline after which a voucher can no longer be claimed. An unclaimed expired voucher can be refunded in full by anyone; if the clinic is no longer active, it can be refunded before expiry. A claimed but unattested one becomes refundable seven days after expiry (the claim grace).

Used in: [How a voucher works — Claimed](protocol/lifecycle.md#claimed), [Contract reference — create_voucher](developers/contracts.md#create_voucher).

## F

### Fee (protocol fee)
The percentage the protocol takes at settlement, expressed in basis points and capped at 1000 bps (10%) at initialisation. Production is 100 bps (1%). Refunds and funding carry no fee.

Used in: [Economics](protocol/economics.md), [Contract reference — settle](developers/contracts.md#settle).

### Funder
The person or wallet that pays USDC into the contract to create a voucher, bound to a specific clinic, service and expiry. The funder can dispute a claim and receives the money back if the voucher is refunded.

Used in: [Funding care for someone](guides/funders.md), [How a voucher works](protocol/lifecycle.md).

## P

### Permissionless
A call that requires no authorisation — anyone may submit it. `settle` and `refund` are permissionless so a clinic never has to chase the funder or an operator to be paid.

Used in: [How a voucher works — Settled](protocol/lifecycle.md#settled), [Economics — who pays what](protocol/economics.md#who-pays-what).

### PHI (protected health information)
Names, dates of birth, phone numbers, addresses, diagnoses, symptoms, test results, prescriptions or any free-text clinical field. None of it is stored on chain, and adding a field for it would be rejected in review.

Used in: [Privacy](protocol/privacy.md).

### Provider
See [Clinic](#clinic-provider). The on-chain name for the clinic that receives settlement.

## R

### Reason code (`reason_code`)
A number attached to a dispute describing why the funder objects: 1 = the patient was not seen, 2 = care was not delivered as described, 3 = a different service was provided, 4 = something else. The contract stores and emits it without interpreting; new codes may be added but existing codes are never renumbered.

Used in: [How a voucher works — Disputed](protocol/lifecycle.md#disputed).

### Registry
The contract that records who is allowed to do what: clinics (`Provider`), their services, attesters and the admin. It answers `is_active_provider` and `is_attester`, which the voucher contract checks on every call.

Used in: [Contract reference — registry](developers/contracts.md#registry).

## S

### Service code
A coarse number identifying a service category, e.g. `101` for "outpatient consult". Codes stay deliberately blunt so the ledger never becomes a public record of what people are being treated for.

Used in: [Privacy — service codes](protocol/privacy.md#service-codes-stay-coarse-on-purpose), [Listing services](guides/clinics.md#listing-services).

### Settle / Settlement
The call that releases escrow to the clinic once the dispute window has closed, takes the protocol fee, and mints a care receipt. It is permissionless and irreversible; a settled voucher cannot be settled or refunded again.

Used in: [How a voucher works — Settled](protocol/lifecycle.md#settled), [Contract reference — settle](developers/contracts.md#settle).

### State machine
The fixed sequence a voucher moves through: `Funded` → `Claimed` → `Attested` → `Settled`, with exits to `Disputed` and `Refunded`. `Settled` and `Refunded` are terminal.

Used in: [How a voucher works](protocol/lifecycle.md), [Contract reference — voucher](developers/contracts.md#voucher).

### Stroops
The smallest unit of XLM (10⁻⁷ XLM), used for Stellar network fees. USDC amounts use the token's own decimals.

Used in: [Economics — network costs](protocol/economics.md#network-costs).

## T

### Terminal state
`Settled` or `Refunded` — nothing leaves them. A voucher cannot be settled or refunded twice.

Used in: [How a voucher works](protocol/lifecycle.md).

### Trust-minimised
The protocol is trust-minimised, not trustless: it narrows the room to cheat (separate attester, funder dispute window, admin resolution, narrow fund routing) but cannot eliminate off-chain trust, since no contract can observe whether a nurse actually examined a patient.

Used in: [Trust model](protocol/trust.md).

### Two-step admin handover
Admin rotation happens in two calls: `propose_admin` by the current admin, then `accept_admin` signed by the nominee. Nothing changes until the nominee signs, so a mistyped address cannot strand dispute resolution.

Used in: [Contract reference — propose_admin / accept_admin](developers/contracts.md#propose_admin-accept_admin).

## U

### USDC
The dollar-pegged token the protocol escrows. Holding it requires an explicit trustline in the wallet; funding, settlement and refunds all move USDC.

Used in: [Funding care for someone — about the trustline](guides/funders.md#about-the-trustline).
