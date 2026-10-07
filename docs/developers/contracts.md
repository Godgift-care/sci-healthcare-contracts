---
title: Contract reference
nav_order: 10
---

# Contract reference

Three contracts. `registry` and `receipt` hold no references and deploy first; `voucher` points at both and deploys last.

All amounts are `i128` in the token's smallest unit. All timestamps are `u64` seconds since epoch.

---

## registry

Answers one question: who is allowed to do what.

### Types

```rust
pub enum ProviderStatus { Pending = 0, Active = 1, Suspended = 2 }

pub struct Provider {
    pub owner: Address,       // receives settlement
    pub name: String,
    pub country: String,      // ISO 3166-1 alpha-2
    pub status: ProviderStatus,
    pub registered_at: u64,
}

pub struct ServiceItem {
    pub code: u32,            // coarse category, never a diagnosis
    pub label: String,
    pub price: i128,
    pub active: bool,
}
```

### Functions

| Function | Parameters | Returns | Auth | Errors |
| --- | --- | --- | --- | --- |
| `initialize` | `admin: Address` | `()` | once | `AlreadyInitialized` |
| `register_provider` | `owner, name, country` | `()` | `owner` | `EmptyName`, `InvalidCountry`, `ProviderExists` |
| `set_provider_status` | `admin, provider_addr, status` | `()` | `admin` | `NotAuthorized`, `ProviderNotFound` |
| `upsert_service` | `provider_addr, code, label, price` | `()` | `provider_addr` | `InvalidPrice`, `EmptyName`, `ProviderNotActive` |
| `remove_service` | `provider_addr, code` | `()` | `provider_addr` | `ServiceNotFound` |
| `add_attester` | `admin, attester` | `()` | `admin` | `NotAuthorized` |
| `remove_attester` | `admin, attester` | `()` | `admin` | `NotAuthorized` |
| `propose_admin` | `admin, new_admin` | `()` | `admin` | `NotAuthorized` |
| `accept_admin` | `new_admin` | `()` | `new_admin` | `NoPendingAdmin`, `NotAuthorized` |
| `get_pending_admin` | — | `Option<Address>` | view | — |
| `get_provider` | `provider_addr` | `Provider` | view | `ProviderNotFound` |
| `is_active_provider` | `provider_addr` | `bool` | view | — |
| `get_service` | `provider_addr, code` | `ServiceItem` | view | `ServiceNotFound` |
| `get_services` | `provider_addr, codes: Vec<u32>` | `Vec<ServiceItem>` | view | — |
| `get_service_price` | `provider_addr, code` | `i128` | view | — |
| `is_attester` | `addr` | `bool` | view | — |
| `get_admin` | — | `Address` | view | `NotInitialized` |

`get_service_price` returns `0` when the service is absent or inactive. It exists so `voucher` can validate pricing over an interface of primitives only, without the two contracts sharing types.

### Errors

| # | Variant |
| --- | --- |
| 1 | `AlreadyInitialized` |
| 2 | `NotInitialized` |
| 3 | `NotAuthorized` |
| 4 | `ProviderNotFound` |
| 5 | `ProviderExists` |
| 6 | `ProviderNotActive` |
| 7 | `ServiceNotFound` |
| 8 | `InvalidPrice` |
| 9 | `InvalidCountry` |
| 10 | `EmptyName` |
| 11 | `NoPendingAdmin` |

### Events

`provider_registered`, `provider_status_changed`, `service_upserted`, `service_removed`, `attester_added`, `attester_removed`, `admin_changed`, `initialized`

---

## voucher

Escrow and the voucher state machine.

### Types

```rust
pub enum VoucherStatus {
    Funded = 0, Claimed = 1, Attested = 2,
    Settled = 3, Disputed = 4, Refunded = 5,
}

pub struct Voucher {
    pub id: u64,
    pub funder: Address,
    pub beneficiary_ref: BytesN<32>,   // opaque, no PHI
    pub provider: Address,
    pub service_code: u32,
    pub amount: i128,
    pub status: VoucherStatus,
    pub created_at: u64,
    pub expires_at: u64,
    pub claimed_at: u64,
    pub attested_at: u64,
    pub dispute_deadline: u64,
}
```

### Functions

| Function | Parameters | Returns | Auth |
| --- | --- | --- | --- |
| `initialize` | `admin, registry, receipt_book, token, dispute_window, fee_bps, fee_account` | `()` | once |
| `create_voucher` | `funder, beneficiary_ref, provider, service_code, amount, expires_at` | `u64` | `funder` |
| `claim` | `provider, voucher_id` | `()` | `provider` |
| `attest` | `attester, voucher_id` | `()` | `attester` |
| `dispute` | `funder, voucher_id, reason_code` | `()` | `funder` |
| `settle` | `voucher_id` | `()` | **none** |
| `refund` | `voucher_id` | `()` | **none** |
| `resolve_dispute` | `admin, voucher_id, refund_funder: bool` | `()` | `admin` |
| `propose_admin` | `admin, new_admin` | `()` | `admin` |
| `accept_admin` | `new_admin` | `()` | `new_admin` |
| `get_voucher` | `voucher_id` | `Voucher` | view |
| `refundable_at` | `voucher_id` | `u64` | view |
| `get_pending_admin` | — | `Option<Address>` | view |
| `get_config` | — | `Config` | view |
| `next_voucher_id` | — | `u64` | view |
| `quote` | `amount` | `(i128, i128)` | view |

`fee_bps` is capped at 1000 (10%) at initialisation.

`settle` and `refund` take no authorisation deliberately: a clinic that has delivered attested care must never depend on the funder or an operator to be paid.

### What each function checks

**`create_voucher`** — amount > 0; expiry in the future; provider is `Active`; the service exists and is active; amount ≥ listed price. Transfers the token *before* writing the voucher, so a failed transfer reverts everything.

**`claim`** — caller is the named provider; provider is still `Active` in the registry; status is `Funded`; not expired.

**`attest`** — caller is a registry attester; caller is **not** the provider; status is `Claimed`. Sets `dispute_deadline = now + dispute_window`.

**`dispute`** — caller is the funder; status is `Claimed`, or `Attested` and before the deadline.

**`settle`** — status is `Attested`; now ≥ `dispute_deadline`. Pays provider, takes fee, mints a receipt.

**`refund`** — status is `Funded` and now ≥ `expires_at`, or status is `Claimed` and now ≥ `expires_at + CLAIM_GRACE_SECS` (7 days). Returns the full amount, no fee. `refundable_at` returns the exact time.

**`propose_admin` / `accept_admin`** — two-step handover, identical in all three contracts. Proposing again replaces the nominee; nothing changes until the nominee signs `accept_admin`.

**`resolve_dispute`** — caller is admin; status is `Disputed`. Routes to funder or provider; no third destination exists.

### Errors

| # | Variant | # | Variant |
| --- | --- | --- | --- |
| 1 | `AlreadyInitialized` | 9 | `InvalidExpiry` |
| 2 | `NotInitialized` | 10 | `InvalidStatus` |
| 3 | `NotAuthorized` | 11 | `VoucherExpired` |
| 4 | `VoucherNotFound` | 12 | `NotYetExpired` |
| 5 | `ProviderNotActive` | 13 | `DisputeWindowOpen` |
| 6 | `ServiceNotOffered` | 14 | `DisputeWindowClosed` |
| 7 | `AmountBelowPrice` | 15 | `InvalidFee` |
| 8 | `InvalidAmount` | 16 | `MathOverflow` |
|  |  | 17 | `NoPendingAdmin` |

### Events

| Event | Topics | Data |
| --- | --- | --- |
| `voucher_created` | `funder`, `provider`, `beneficiary_ref` | `voucher_id`, `service_code`, `amount`, `created_at`, `expires_at` |
| `voucher_claimed` | `provider` | `voucher_id`, `claimed_at` |
| `voucher_attested` | `attester` | `voucher_id`, `attested_at`, `dispute_deadline` |
| `voucher_disputed` | `funder` | `voucher_id`, `reason_code` |
| `voucher_settled` | `provider` | `voucher_id`, `net`, `fee` |
| `voucher_refunded` | `funder` | `voucher_id`, `amount` |
| `dispute_resolved` | `admin` | `voucher_id`, `refunded_funder` |

> Fields listed under **Topics** are published in the event's topic list, **not** in its data map. An indexer reading them off the data map gets `undefined`.

---

## receipt

A non-transferable record that a care episode was funded and settled. There is deliberately no transfer function.

### Types

```rust
pub struct Receipt {
    pub voucher_id: u64,
    pub beneficiary_ref: BytesN<32>,
    pub provider: Address,
    pub service_code: u32,
    pub amount: i128,
    pub settled_at: u64,
}
```

### Functions

| Function | Parameters | Returns | Auth |
| --- | --- | --- | --- |
| `initialize` | `admin, minter` | `()` | once |
| `set_minter` | `admin, minter` | `()` | `admin` |
| `mint` | `minter, voucher_id, beneficiary_ref, provider, service_code, amount` | `()` | `minter` |
| `get_receipt` | `voucher_id` | `Receipt` | view |
| `count_for` | `beneficiary_ref` | `u32` | view |
| `get_minter` | — | `Address` | view |
| `get_admin` | — | `Address` | view |

Only the minter — the voucher contract — may mint. Because the voucher contract does not exist when the receipt book is initialised, deploy order is: receipt, voucher, then `set_minter`.

### Errors

| # | Variant |
| --- | --- |
| 1 | `AlreadyInitialized` |
| 2 | `NotInitialized` |
| 3 | `NotAuthorized` |
| 4 | `ReceiptNotFound` |
| 5 | `ReceiptExists` |
| 6 | `InvalidAmount` |
| 7 | `NoPendingAdmin` |

### Events

`receipt_minted` — topics `beneficiary_ref`, `provider`; data `voucher_id`, `service_code`, `amount`, `settled_at`

## faucet (testnet only)

Lets anyone trying the demo get test USDC without a server holding the issuer key. The demo token's Stellar Asset Contract names the faucet as its admin, so `drip` can mint. **Never point it at a real asset.** `scripts/deploy-faucet.sh` refuses mainnet.

| Function | Parameters | Returns | Auth |
| --- | --- | --- | --- |
| `initialize` | `admin, token, amount, cooldown` | `()` | once |
| `drip` | `to` | `i128` | `to` |
| `set_drip` | `admin, amount, cooldown` | `()` | `admin` |
| `release_token_admin` | `admin, new_admin` | `()` | `admin` |
| `get_config` | — | `FaucetConfig` | view |
| `next_drip_at` | `to` | `u64` (0 if allowed now) | view |

`drip` requires the recipient's signature, so one wallet cannot drain the faucet into addresses it does not control, and each address may drip once per `cooldown`. A classic account needs a trustline for the token first; the token reports a missing one as its error `#13`.

On testnet the faucet drips 50 USDC once a day: [`CCB25WG7DQ2CSNXPTOHZO2PDZ324FQPZLH3LSNCCLWYM3FEH52GEXC47`](https://stellar.expert/explorer/testnet/contract/CCB25WG7DQ2CSNXPTOHZO2PDZ324FQPZLH3LSNCCLWYM3FEH52GEXC47).

| # | Error |
| --- | --- |
| 1 | `AlreadyInitialized` |
| 2 | `NotInitialized` |
| 3 | `NotAuthorized` |
| 4 | `InvalidAmount` |
| 5 | `CoolingDown` |
