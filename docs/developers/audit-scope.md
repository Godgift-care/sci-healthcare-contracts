---
title: Security audit scope and preparation
nav_order: 14
---

# Security audit scope and preparation

This document defines what an audit of the SCI Healthcare contracts should cover. It is written for the audit team so the first week is spent testing, not writing the scope.

## Contracts and commit in scope

The audit covers the Soroban smart contracts in this repository at a specific commit.

| Contract | Crate | Path | Purpose |
| --- | --- | --- | --- |
| **Registry** | `sci-registry` | `contracts/registry` | Provider registration, service catalogue, attester set, status lifecycle |
| **Voucher** | `sci-voucher` | `contracts/voucher` | Escrow, voucher state machine (Funded → Claimed → Attested → Settled/Refunded/Disputed), fee logic, dispute resolution |
| **Receipt Book** | `sci-receipt` | `contracts/receipt` | Non-transferable care receipts minted on settlement |
| **Sponsor Desk** | `sci-sponsor` | `contracts/sponsor` | Second-voucher matching funded by a sponsor's deposited budget |
| **Faucet** | `sci-faucet` | `contracts/faucet` | Testnet-only USDC minting for demos; not deployed on mainnet |

**In-scope commit**: the commit this document is reviewed against (recorded in the audit engagement letter). All contracts are built from the same workspace commit; `version()` on each contract returns the workspace version.

## Trust assumptions

These are design decisions documented so they are not mistaken for bugs. See [the trust model](../protocol/trust.md) for the full rationale.

1. **Attesters are trusted** to confirm delivery honestly. The contract refuses attestation from the provider being paid, and the funder gets a dispute window, but a colluding attester and provider can still settle a voucher for care that did not happen.
   - *Mitigations in code*: per-attester daily cap (`DEFAULT_MAX_ATTESTATIONS_PER_DAY = 100`, admin-settable via `set_attestation_cap`), admin revocation, funder dispute window, permanent on-chain attribution.

2. **The admin resolves disputes**. The admin can route escrowed funds to either the funder or the provider, and can suspend providers. The admin cannot divert funds anywhere else.
   - *Mitigations in code*: two-step admin handover (`propose_admin` / `accept_admin`), admin actions restricted to `Disputed` vouchers only, no minting or direct token control.

3. **Beneficiary references are pseudonymous, not anonymous**. Anyone who already knows both a patient identifier and the HMAC key can confirm a match.
   - *Mitigation*: the reference is a salted SHA-256 hash; no PHI is stored on chain.

4. **The settlement token issuer can claw back or freeze**. USDC is a regulated stablecoin; its issuer can reach the voucher contract's escrow balance. The contract behaves predictably when this happens: `EscrowShortfall` (#23) and `TokenTransferFailed` (#24) revert the whole call so nothing is half-paid.
   - *Tests*: `clawback_*`, `partial_clawback_*`, `frozen_*` in `contracts/voucher/src/test.rs`.

5. **Registry verification is off-chain**. The registry only records the outcome of a human check (licence, site visit). If a clinic is verified that should not have been, funders can send money to it.

## Invariants

Each invariant below is a property that must hold for every execution. The test that checks it is linked so the audit team can run it and inspect the implementation.

### Registry invariants

| # | Invariant | Test |
| --- | --- | --- |
| R1 | A provider can only be registered once. Duplicate registration is rejected. | `duplicate_registration_rejected` |
| R2 | Country code must be exactly two ASCII uppercase letters (ISO 3166-1 alpha-2). | `bad_country_code_rejected` |
| R3 | Provider name cannot be empty. | `empty_name_rejected` |
| R4 | Only the admin can change provider status. | `non_admin_cannot_change_status` |
| R5 | Status transitions are restricted to the documented matrix (Pending→Active, Pending→Suspended, Active→Suspended, Suspended→Active). No backward moves, no self-transitions. | `invalid_transition_rejected` (in `contracts/registry/src/test.rs`) |
| R6 | Retired is terminal: nothing — not even the admin — can move a provider out of `Retired`. | `retire_is_terminal` |
| R7 | Only an `Active` provider may list or update services. | `inactive_provider_cannot_upsert_service` |
| R8 | Price increases require 24-hour notice via `schedule_service_price`; decreases and new services apply immediately. | `price_increase_needs_notice`, `scheduled_price_rise_applies_at_effective_time` |
| R9 | A paused service is not fundable (`is_service_available` returns `false`), but vouchers already funded are unaffected. | `pausing_a_service_does_not_affect_funded_vouchers` |
| R10 | Only an authorised attester may attest; the registry check is enforced by the voucher contract. | `non_attester_cannot_attest`, `revoked_attester_cannot_attest` |
| R11 | `list_providers` and `providers_in` are paginated and capped at `MAX_PROVIDERS_PAGE` (50). | `list_providers_pages`, `providers_in_pages` |
| R12 | Admin handover is two-step: `propose_admin` then `accept_admin`; a typo cannot strand the contract. | `admin_handover_takes_two_steps`, `only_the_nominee_can_accept_admin` |
| R13 | Reason codes are stored on the provider and emitted in `provider_status_changed`; they are added, never renumbered. | `reason_code_is_stored_and_emitted` |

### Voucher invariants

| # | Invariant | Test |
| --- | --- | --- |
| V1 | **Funds can only reach the named clinic or return to the funder.** No third destination exists in any code path. | Verified by inspection of `pay_out`, `pay_back`, `pay_split` — all destinations are `voucher.provider`, `voucher.funder`, `config.fee_account`, or `voucher.attester` (which is the attester of record). |
| V2 | **The clinic being paid cannot attest its own delivery.** The contract explicitly checks `attester == voucher.provider` and rejects with `NotAuthorized`. | `provider_cannot_attest_its_own_payment` |
| V3 | **A clinic cannot be paid without an attestation.** No path exists from `Funded` or `Claimed` straight to `Settled`. | `cannot_settle_unattested_voucher`, `settle_requires_attestation` |
| V4 | **Money cannot be released early.** `settle` fails with `DisputeWindowOpen` while the dispute window is open. | `cannot_settle_during_dispute_window` |
| V5 | **An unattested voucher always returns to the funder.** Unclaimed: refundable at expiry (or immediately if clinic inactive). Claimed but never attested: refundable 7 days after expiry (`CLAIM_GRACE_SECS`). No one can prevent this — `refund` needs no authorisation. | `expired_unclaimed_voucher_refunds_in_full`, `funded_voucher_of_suspended_clinic_refunds_immediately`, `claimed_but_never_attested_voucher_refunds_after_grace` |
| V6 | **The full amount is refunded.** No fee is taken on care that did not happen. | `expired_unclaimed_voucher_refunds_in_full` (asserts `fee_account` balance is 0) |
| V7 | **A settled voucher cannot be settled or refunded again.** Terminal states are terminal. | `cannot_settle_twice`, `cannot_refund_settled_voucher` |
| V8 | **Admin rights cannot be handed to a mistyped address.** Every contract transfers administration in two steps — `propose_admin`, then `accept_admin` signed by the new address. | `admin_handover_takes_two_steps`, `only_the_nominee_can_accept_admin`, `accept_admin_without_proposal_errors` |
| V9 | **Fee is capped at 1000 bps (10%) at initialisation.** | `excessive_fee_rejected_at_init` |
| V10 | **Attester share cannot exceed the fee.** `attester_share_bps ≤ fee_bps` enforced at init. | `excessive_attester_share_rejected_at_init` |
| V11 | **Quote matches settlement exactly.** `quote(amount)` returns `(fee, net)` where `fee + net = amount`, and settlement pays exactly those amounts. | `quote_matches_settlement`, `quote_equals_settlement_for_every_amount` |
| V12 | **Escrow accounting is correct.** `escrow_summary` returns `(open_escrow, token_balance)` and the two are equal when accounting is correct. | `escrow_summary_tracks_the_settlement_lifecycle`, `escrow_summary_tracks_a_refund`, `escrow_summary_tracks_each_voucher_and_dispute_resolution` |
| V13 | **Per-attester daily cap limits blast radius of a stolen key.** Default 100/day, admin-settable. Counter resets at UTC midnight. | `attestation_cap_defaults_and_counts_down`, `attestation_cap_is_enforced_and_resets_next_utc_day`, `attestation_cap_is_per_attester` |
| V14 | **Dispute window bounds enforce a floor and ceiling.** `1 day ≤ min ≤ max ≤ 30 days`. Per-voucher window must fall within bounds. | `dispute_window_bounds_enforced`, `per_voucher_dispute_window_within_bounds` |
| V15 | **Per-clinic escrow cap limits exposure.** Admin-settable `max_escrow_per_provider`; `0` = unlimited. Affects new vouchers only. | `escrow_cap_limits_new_vouchers`, `lowered_cap_does_not_affect_existing_vouchers` |
| V16 | **Presence commitment requires proof to claim.** A voucher funded with `presence_commitment` can only be claimed with `claim_with_proof(secret)` where `sha256(secret) == commitment`. | `presence_commitment_requires_proof`, `invalid_presence_proof_rejected` |
| V17 | **Redirect only works for funded vouchers to active providers offering the same service at ≤ the escrowed amount.** | `redirect_moves_a_funded_voucher_and_emits_event`, `cannot_redirect_claimed_voucher`, `cannot_redirect_to_inactive_provider`, `cannot_redirect_when_new_price_exceeds_amount` |
| V18 | **Reduce amount only for funder, only while `Funded`, only down to current listed price (at call time), and only if service still listed.** | `reduce_to_the_listed_price_succeeds`, `cannot_reduce_below_the_listed_price`, `cannot_reduce_after_claimed`, `cannot_reduce_after_settled`, `only_the_funder_can_reduce_the_amount`, `reduce_uses_the_price_at_call_time`, `cannot_reduce_when_service_no_longer_offered` |
| V19 | **Settlement splits fee correctly: clinic gets `amount - fee`, attester gets `amount * attester_share_bps / 10000`, fee account gets the rest. Three payouts sum to `amount` exactly.** | `settlement_splits_the_fee_between_attester_and_fee_account`, `zero_attester_share_behaves_as_before`, `attester_share_equal_to_the_fee_pays_it_all_to_the_attester` |
| V20 | **Clawback and freeze are handled safely.** If the token issuer claws back escrow, `EscrowShortfall` is returned; if the issuer freezes the contract or a recipient, `TokenTransferFailed` is returned. In both cases the whole call reverts — nothing is half-paid. | `clawback_before_settle_fails_with_escrow_shortfall`, `partial_clawback_settles_what_remains`, `frozen_escrow_fails_with_token_transfer_failed`, `frozen_provider_fails_with_token_transfer_failed` |
| V21 | **Voucher IDs increment sequentially.** | `voucher_ids_increment` |
| V22 | **Funder need not be the beneficiary.** A relative abroad can fund care for someone at home. | `funder_need_not_be_beneficiary` |
| V23 | **Claim grace period protects funders from a claim that never gets attested.** 7 days after expiry, the funder can refund a claimed-but-unattested voucher. | `claimed_but_never_attested_voucher_refunds_after_grace` |
| V24 | **Suspension does not shorten claim grace.** Care may already have been given before suspension; the attester still has the full grace period. | `claimed_voucher_of_suspended_clinic_keeps_claim_grace` |
| V25 | **Dispute resolution by admin routes only to funder or provider.** `resolve_dispute(refund_funder=true)` refunds funder; `false` pays provider with fee. | `resolving_for_funder_refunds`, `resolving_for_provider_pays_out` |
| V26 | **Split resolution allows any split 0..amount.** 0 = full refund (no receipt), amount = full payout. Fee on the provider portion only. | `split_zero_refunds_funder_in_full_without_a_receipt`, `split_half_pays_clinic_fee_and_funder_exactly`, `split_full_amount_matches_a_full_payout` |
| V27 | **Admin cannot resolve an undisputed voucher.** | `cannot_resolve_undisputed_voucher` |
| V28 | **Storage TTL extension keeps active entries alive.** Instance entries extended on every write (threshold 7 days → 30 days). Voucher/Provider/Receipt entries extended on every read/write (threshold 30 days → 90 days). | `live_voucher_survives_sixty_days_of_activity`, `settled_voucher_and_its_receipt_survive_sixty_days_of_activity`, `contract_instance_survives_sixty_days_of_writes`, `writes_extend_the_contract_instance`, `provider_service_and_attester_survive_sixty_days_of_activity` |
| V29 | **Lapsed entries are restorable, not lost.** A read on an archived entry restores it with network minimum TTL. | `abandoned_voucher_is_restored_and_still_refunds`, `registry_reads_restore_a_lapsed_entry`, `receipt_read_restores_a_lapsed_entry` |

### Receipt invariants

| # | Invariant | Test |
| --- | --- | --- |
| RC1 | Only the minter (voucher contract) may mint. | `only_minter_can_mint` |
| RC2 | Minter handover is two-step: `propose_minter` then `accept_minter`. | `minter_handover_takes_two_steps`, `only_nominee_can_accept_minter` |
| RC3 | Receipts are non-transferable (soulbound). No transfer function exists. | Verified by inspection: no `transfer` entry point. |
| RC4 | `count_for(beneficiary_ref)` returns the number of settled episodes for that beneficiary. | `count_for_increments_on_settlement` |
| RC5 | `provider_stats` returns `(count, total_amount)` with checked arithmetic; overflow is refused with `MathOverflow`. | `provider_stats_uses_checked_arithmetic` |
| RC6 | Receipts are indexed per provider for on-chain track record. | `receipts_of_provider_returns_indexed_receipts` |
| RC7 | Storage TTL extension same as voucher. | `receipt_survives_sixty_days_of_activity` |

### Sponsor invariants

| # | Invariant | Test |
| --- | --- | --- |
| S1 | A match is an ordinary voucher funded by the desk, going through the same lifecycle. | `match_creates_an_ordinary_voucher` |
| S2 | Sponsor cannot match its own voucher or a match the desk funded. | `self_match_not_allowed` |
| S3 | Match amount = `voucher_amount * match_bps / 10000`, capped at `cap_per_voucher`. | `match_amount_is_calculated_and_capped` |
| S4 | Match only qualifies if clinic country and service code are in sponsor's rules. | `country_not_eligible`, `service_not_eligible` |
| S5 | Match only qualifies if voucher is old enough and large enough. | `voucher_too_new`, `voucher_too_small` |
| S6 | Match requires available budget; `withdraw` only allows unused budget. | `budget_exhausted_rejects_match`, `withdraw_exceeds_budget_rejected` |
| S7 | Max 20 open matches per sponsor at once. | `too_many_open_matches_are_refused` |
| S8 | `reconcile_sponsor` (permissionless) updates books: settled matches stop counting, refunded matches return to budget. | `reconcile_updates_books_after_settle`, `reconcile_returns_refunded_to_budget` |
| S9 | Sponsor admin handover is two-step. | `admin_handover_takes_two_steps` |

### Faucet invariants (testnet only)

| # | Invariant | Test |
| --- | --- | --- |
| F1 | `drip` requires recipient's signature; one wallet cannot drain into uncontrolled addresses. | `drip_requires_the_recipients_signature` |
| F2 | Cooldown enforced per address. | `cooldown_is_per_address` |
| F3 | Daily cap limits total minted per UTC day across all addresses. | `daily_cap_limits_total_minted_per_day`, `daily_cap_resets_at_day_boundary` |
| F4 | Lifetime cap per address; 0 = unlimited. | `lifetime_cap_is_per_address`, `lifetime_cap_is_enforced_across_days`, `zero_lifetime_cap_means_unlimited` |
| F5 | Admin can pause without giving up token admin. | `paused_faucet_rejects_drips` |
| F6 | `release_token_admin` hands token admin back in two steps. | `token_admin_can_be_handed_back` |
| F7 | Scripts refuse mainnet deployment. | Verified in `scripts/deploy-faucet.sh` |

## Known limitations

1. **No attester quorum for high-value vouchers.** A single attester can confirm delivery; a colluding attester+provider can drain vouchers. An attester quorum is an open issue, not yet built.

2. **Admin key is a single point of failure today.** Mainnet must deploy with a 2-of-3 multisig admin (see `docs/developers/multisig-admin.md`).

3. **Registry verification is off-chain.** The contract only records the outcome; a fraudulent verification cannot be detected on-chain.

4. **No on-chain identity verification.** Beneficiary references are pseudonymous hashes; the protocol does not verify who the patient is.

5. **Quality of care is not enforced.** The protocol only guarantees an attester said care happened, not that it was good care.

6. **No ceiling on clinic prices.** The protocol enforces a floor (listed price), not a ceiling.

7. **Faucet is testnet-only.** It must never be pointed at a real asset; `scripts/deploy-faucet.sh` refuses mainnet.

8. **Contracts have no pause or upgrade function.** Rollback means containing the incident and moving users to a reviewed replacement deployment.

9. **Indexer is off-chain.** The protocol assumes an indexer for app UX; on-chain views exist for auditability but are not a full replacement.

10. **Stellar asset contract clawback/freeze.** The token issuer can reach escrowed funds; the contract handles it safely but cannot prevent it.

## Out of scope

- Anything requiring the admin key to be compromised (the trust model documents admin powers explicitly).
- Attesters confirming care that did not happen — this is a known and documented trust assumption, not a vulnerability. Reports that *narrow* it are welcome.
- Denial of service against public Soroban RPC endpoints.
- Missing rate limits on read-only endpoints already behind a rate limiter.
- Testnet-only misconfiguration.
- Frontend, indexer, backend services, or off-chain infrastructure.
- Operational security of the deployment environment (key custody, network access, monitoring).
- Cryptographic primitives provided by the Stellar platform (ed25519, SHA-256, etc.).
- The Stellar consensus protocol or Soroban host itself.

## How to run the tests

```bash
# All contracts
cargo test --workspace

# Specific contract
cargo test -p sci-voucher
cargo test -p sci-registry
cargo test -p sci-receipt
cargo test -p sci-sponsor
cargo test -p sci-faucet
```

Tests are deterministic, use mocked auth (`env.mock_all_auths()`), and run offline (no network). Each test is named after the invariant it verifies.

## What breaks if this is wrong

If an invariant is missing or incorrectly documented, the audit team will not test for it. If a test is missing for an invariant, a regression could ship undetected. If the scope omits a contract, its code will not be reviewed.