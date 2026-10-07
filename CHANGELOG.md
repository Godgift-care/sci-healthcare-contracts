# Changelog

## Unreleased

Redeployed to testnet on 2026-10-07; addresses are in `deployments/testnet.env`.

### Fixed
- **voucher:** a `Claimed` voucher that is never attested no longer locks the funder's money forever. `refund` accepts it seven days (`CLAIM_GRACE_SECS`) after expiry.
- **voucher:** a provider suspended after a voucher was funded can no longer `claim` it.
- **voucher:** every state transition extends the instance TTL, not only `create_voucher`.
- **registry:** attester entries are TTL-extended on read like providers and services.

### Added
- Two-step admin handover (`propose_admin` / `accept_admin` / `get_pending_admin`) in registry, voucher and receipt. Replaces the registry's one-step `set_admin`; the voucher admin previously could not be rotated at all.
- `refundable_at(voucher_id)` view on the voucher contract.
- Documented dispute reason codes 1–4.

Tests: 84 (was 69).

## 0.1.0 — 2026-09-02

Initial registry, voucher escrow and care receipt contracts.
