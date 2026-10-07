<p align="center">
  <img src="docs/banner.png" alt="SCI Healthcare" width="640" />
</p>

<p align="center">
  <a href="https://github.com/Godgift-care/sci-healthcare-contracts/actions/workflows/ci.yml">
    <img src="https://github.com/Godgift-care/sci-healthcare-contracts/actions/workflows/ci.yml/badge.svg" alt="CI" />
  </a>
  <img src="https://img.shields.io/badge/soroban--sdk-27.0.6-blue" alt="soroban-sdk 27.0.6" />
  <img src="https://img.shields.io/badge/rust-1.96-orange" alt="rust 1.96" />
  <img src="https://img.shields.io/badge/license-Apache--2.0-green" alt="Apache 2.0" />
  <img src="https://img.shields.io/badge/tests-84%20passing-brightgreen" alt="84 tests" />
</p>

# SCI Healthcare — Contracts | [Documentation](https://godgift-care.github.io/sci-healthcare-contracts/)

> **Live:** [App](https://sci-healthcare.vercel.app) · [API](https://sci-healthcare-api.onrender.com/stats) · [Docs](https://godgift-care.github.io/sci-healthcare-contracts/) · [Contracts on testnet](https://stellar.expert/explorer/testnet/contract/CBPUQN6CNJCNGPKNEXB7UM4T7RINKQB7HQI5CXEOSQ5EFRHE4G4VKPSQ)
>
> The API runs on Render's free tier and sleeps after ~15 minutes idle; the first
> request may take 30–60 seconds to wake it.

Soroban contracts for prepaid, purpose-bound care vouchers on Stellar.

Someone funds a voucher for a specific clinic and a specific service. The money sits in escrow and is released only once an independent attester confirms the care was delivered. If it never happens, the funder gets it back. The funder does not have to be the patient — a relative sending money from abroad uses exactly the same path, which is the point: a remittance that can only be spent on the care it was sent for.

**No patient health information ever touches the ledger.** Vouchers carry a coarse service category and an opaque beneficiary reference, never a diagnosis, a result, or a name.

## Maintainers

<table align="center">
  <tr>
    <td align="center">
      <img src="https://github.com/adelekevat.png" width="140" alt="Maintainer" />
      <br /><br />
      <strong>Adeleke | Protocol &amp; Contracts</strong>
      <br /><br />
      <a href="https://github.com/adelekevat">adelekevat</a>
    </td>
  </tr>
</table>

## Deployed on Stellar Testnet

| Contract | Address |
| --- | --- |
| Registry | [`CAMU635NRFEHATJ6MTY5SI43IAZ43KHPFXAQDQAST2AHKCHLSGWHVE4V`](https://stellar.expert/explorer/testnet/contract/CAMU635NRFEHATJ6MTY5SI43IAZ43KHPFXAQDQAST2AHKCHLSGWHVE4V) |
| Voucher escrow | [`CBPUQN6CNJCNGPKNEXB7UM4T7RINKQB7HQI5CXEOSQ5EFRHE4G4VKPSQ`](https://stellar.expert/explorer/testnet/contract/CBPUQN6CNJCNGPKNEXB7UM4T7RINKQB7HQI5CXEOSQ5EFRHE4G4VKPSQ) |
| Care receipts | [`CBCJRI2BCZBMZNX77WWDC2NGSV4ZNAN5Q23XHYNBH6UTOQMPJHZGX5C7`](https://stellar.expert/explorer/testnet/contract/CBCJRI2BCZBMZNX77WWDC2NGSV4ZNAN5Q23XHYNBH6UTOQMPJHZGX5C7) |
| Test USDC (SAC) | [`CCRLJ7FEIKTR3GBT3VQQPSIJGRBO2WPNMVFKJDP6AYGJQWECOR4QVE5M`](https://stellar.expert/explorer/testnet/contract/CCRLJ7FEIKTR3GBT3VQQPSIJGRBO2WPNMVFKJDP6AYGJQWECOR4QVE5M) |

## Architecture

Three contracts, one responsibility each.

```
  registry ──────────┐         who may bill, for what, and who may attest
  (providers,        │
   services,         ├──────>  voucher  ──────>  receipt
   attesters)        │        (escrow +         (non-transferable
                     │         lifecycle)        care record)
  SEP-41 token ──────┘
  (USDC)
```

`registry` and `receipt` hold no references and deploy first. `voucher` points at both and deploys last.

### Voucher lifecycle

```
                 claim              attest             settle
  Funded ──────────────> Claimed ───────────> Attested ────────> Settled
    │                       │                     │
    │ refund (past expiry)  │ dispute             │ dispute
    ▼                       ▼                     ▼
 Refunded  <───────────  Disputed  ──── resolve ──┴──> Settled
    ▲                       │
    └───────────────────────┘ refund (claimed, never attested, 7 days past expiry)
```

`settle` and `refund` are **permissionless** — a clinic never depends on the funder or an operator to get paid.

## Trust model, stated plainly

This is trust-*minimised*, not trustless. Whether care actually happened is an off-chain fact. The protocol narrows the room to cheat; it does not eliminate it.

- The contract **refuses an attestation from the provider being paid**, even if that provider holds an attester role. Separating who gets paid from who confirms delivery is the main check here.
- The funder gets a dispute window after attestation, before any money moves.
- Dispute resolution is a trusted human decision, scoped as narrowly as possible: the admin can only route already-escrowed funds to one of the two parties, never divert them elsewhere.

## Quick start

### Install Rust and the Stellar CLI

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add wasm32v1-none
cargo install --locked stellar-cli
```

### Build and test

```bash
git clone https://github.com/Godgift-care/sci-healthcare-contracts
cd sci-healthcare-contracts
cargo test                                    # 84 tests
cargo build --target wasm32v1-none --release  # three .wasm artifacts
```

### Deploy to testnet

```bash
stellar keys generate --network testnet --fund sci-admin
stellar keys generate --network testnet --fund sci-issuer
./scripts/deploy.sh testnet     # deploys in dependency order and wires them up
./scripts/seed.sh testnet       # demo clinic, services, attester, vouchers
```

`deploy.sh` writes every address to `deployments/testnet.env` and prints a copy-pasteable block for the app's environment.

## Contract reference

### `registry`

| Function | Auth | Purpose |
| --- | --- | --- |
| `initialize(admin)` | once | Sets the administrator |
| `register_provider(owner, name, country)` | owner | Self-registers a clinic as `Pending` |
| `set_provider_status(admin, provider, status)` | admin | `Pending` → `Active` → `Suspended` |
| `upsert_service(provider, code, label, price)` | provider | Lists a billable service; requires `Active` |
| `remove_service(provider, code)` | provider | Delists a service |
| `add_attester(admin, attester)` | admin | Grants attester rights |
| `remove_attester(admin, attester)` | admin | Revokes them |
| `propose_admin(admin, new_admin)` | admin | Nominates a new admin |
| `accept_admin(new_admin)` | nominee | Completes the two-step handover |
| `is_active_provider(provider)` | view | The check `voucher` relies on |
| `get_service_price(provider, code)` | view | Price, or 0 if absent/inactive |
| `is_attester(addr)` | view | Attester check |

### `voucher`

| Function | Auth | Purpose |
| --- | --- | --- |
| `initialize(admin, registry, receipt_book, token, dispute_window, fee_bps, fee_account)` | once | Fixes protocol config; fee capped at 10% |
| `create_voucher(funder, beneficiary_ref, provider, service_code, amount, expires_at)` | funder | Escrows funds against one service at one active provider |
| `claim(provider, voucher_id)` | provider | Marks the patient as presented |
| `attest(attester, voucher_id)` | attester | Confirms delivery; opens the dispute window |
| `dispute(funder, voucher_id, reason_code)` | funder | Contests before the window closes |
| `settle(voucher_id)` | none | Releases escrow once the window closes |
| `refund(voucher_id)` | none | Returns escrow after an unclaimed voucher expires, or 7 days after expiry if claimed but never attested |
| `refundable_at(voucher_id)` | view | When `refund` will succeed |
| `propose_admin` / `accept_admin` | admin / nominee | Two-step admin handover |
| `resolve_dispute(admin, voucher_id, refund_funder)` | admin | Routes escrow to one of the two parties |
| `quote(amount)` | view | `(fee, net)` so a clinic sees exactly what lands |

### `receipt`

| Function | Auth | Purpose |
| --- | --- | --- |
| `initialize(admin, minter)` | once | Sets admin and minter |
| `set_minter(admin, minter)` | admin | Repoints at a redeployed voucher contract |
| `mint(minter, voucher_id, beneficiary_ref, provider, service_code, amount)` | minter | Records a settled episode |
| `count_for(beneficiary_ref)` | view | Settled episodes for a beneficiary |

There is deliberately no transfer function on receipts.

## Related repositories

| Repo | Purpose |
| --- | --- |
| [sci-healthcare-contracts](https://github.com/Godgift-care/sci-healthcare-contracts) | Soroban contracts (this repo) |
| [sci-healthcare-backend](https://github.com/Godgift-care/sci-healthcare-backend) | Event indexer and read API |
| [sci-healthcare-frontend](https://github.com/Godgift-care/sci-healthcare-frontend) | Web app for funders, clinics and attesters |

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) first. Issues labelled `good first issue` are scoped for newcomers. Security reports go through [SECURITY.md](SECURITY.md) — please do not open a public issue for a vulnerability.

## Contributors

<a href="https://github.com/Godgift-care/sci-healthcare-contracts/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=Godgift-care/sci-healthcare-contracts" />
</a>

## License

Apache-2.0. See [LICENSE](LICENSE).

> **Unaudited.** These contracts have not been through a third-party security audit. Do not use them with real funds.
