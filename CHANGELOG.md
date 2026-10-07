# Changelog

## Unreleased

Redeployed to testnet on 2026-10-07; addresses are in `deployments/testnet.env`.

### Fixed
- **voucher:** a `Claimed` voucher that is never attested no longer locks the funder's money forever. `refund` accepts it seven days (`CLAIM_GRACE_SECS`) after expiry.
- **voucher:** a provider suspended after a voucher was funded can no longer `claim` it.
- **voucher:** every state transition extends the instance TTL, not only `create_voucher`.
- **registry:** attester entries are TTL-extended on read like providers and services.

### Added
- **faucet** (testnet only): anyone can get 50 test USDC once a day from the app, with no server holding the issuer key. The demo token's SAC names the faucet as admin; `release_token_admin` hands it back. Deployed by `scripts/deploy-faucet.sh`, which refuses mainnet. 10 tests.
- Two-step admin handover (`propose_admin` / `accept_admin` / `get_pending_admin`) in registry, voucher and receipt. Replaces the registry's one-step `set_admin`; the voucher admin previously could not be rotated at all.
- `refundable_at(voucher_id)` view on the voucher contract.
- Documented dispute reason codes 1–4.

Tests: 94 (was 69).

## 0.1.0 — 2026-09-02

Initial registry, voucher escrow and care receipt contracts.
