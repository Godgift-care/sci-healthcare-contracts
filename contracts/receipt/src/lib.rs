#![no_std]
// A panic aborts the whole invocation with an opaque host error. Contract
// code must return a typed error instead, so both are denied outside tests.
// Tests keep them for brevity: they are fail-fast assertions, not contract
// behaviour.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

//! # SCI Healthcare — Care Receipts
//!
//! A non-transferable record that a specific care episode was funded and
//! settled. Minted only by the voucher contract, only on settlement.
//!
//! Receipts give a patient a portable, verifiable history of care that was
//! *paid for* — useful for proving standing with a clinic or a future
//! financing partner — without putting any clinical information on chain.
//! A receipt records the service category, the provider, the amount and the
//! date. It never records a diagnosis, a result, or a name.
//!
//! There is deliberately no transfer function. These are bound to the
//! beneficiary reference they were minted against.

extern crate alloc;

use sci_common as common;
use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, Address, BytesN, Env,
    String, Vec,
};

/// Typed contract events. Field names are part of the public interface:
/// the indexer decodes these by name.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Initialized {
    pub admin: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MinterProposed {
    #[topic]
    pub admin: Address,
    pub pending_minter: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MinterChanged {
    pub minter: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminProposed {
    #[topic]
    pub admin: Address,
    pub pending_admin: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminChanged {
    pub new_admin: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptMinted {
    #[topic]
    pub beneficiary_ref: BytesN<32>,
    #[topic]
    pub provider: Address,
    pub voucher_id: u64,
    pub service_code: u32,
    pub amount: i128,
    pub settled_at: u64,
}

/// Most receipts one `receipts_of_provider` call may return.
///
/// Caps the response so a clinic with a long history cannot make a single
/// read exceed the host's resource limits; callers page with `start`.
pub const MAX_RECEIPTS_PAGE: u32 = 50;

/// Human-readable name wallets and explorers show for a receipt.
const RECEIPT_NAME: &str = "SCI Care Receipt";
/// Token-style symbol explorers use when listing the receipt.
const RECEIPT_SYMBOL: &str = "SCIREC";
/// Base of the per-receipt URI. Only the voucher id is appended, so the URI
/// carries no patient or beneficiary data.
const RECEIPT_URI_BASE: &str = "https://receipts.sci.health/v1/receipt/";

/// A settled care episode.
///
/// `beneficiary_ref` is an opaque 32-byte reference supplied by the client.
/// It is a salted hash of an off-chain patient identifier: the protocol can
/// verify two receipts belong to the same person, and can verify nothing
/// else about them.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Receipt {
    pub voucher_id: u64,
    pub beneficiary_ref: BytesN<32>,
    pub provider: Address,
    pub service_code: u32,
    pub amount: i128,
    pub settled_at: u64,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    /// The voucher contract, the only address permitted to mint.
    Minter,
    Receipt(u64),
    /// Running count of settled episodes per beneficiary reference.
    Count(BytesN<32>),
    /// Address nominated by `propose_admin`, awaiting `accept_admin`.
    PendingAdmin,
    /// Address nominated by `propose_minter`, awaiting `accept_minter`.
    PendingMinter,
    /// Number of receipts minted for a provider: the provider index length.
    ProviderCount(Address),
    /// A provider's receipt ids in mint order: `(provider, index) -> voucher_id`.
    ProviderReceipt(Address, u32),
}

#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ReceiptError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    NotAuthorized = 3,
    ReceiptNotFound = 4,
    ReceiptExists = 5,
    InvalidAmount = 6,
    NoPendingAdmin = 7,
    NoPendingMinter = 8,
}

/// Maps the shared helpers' failures onto this contract's error numbers, so
/// callers keep seeing the same codes they did before the helpers were shared.
impl From<common::CommonError> for ReceiptError {
    fn from(error: common::CommonError) -> Self {
        match error {
            common::CommonError::NotAuthorized => ReceiptError::NotAuthorized,
            common::CommonError::NoPendingAdmin => ReceiptError::NoPendingAdmin,
        }
    }
}

#[contract]
pub struct ReceiptBook;

#[contractimpl]
impl ReceiptBook {
    /// Sets the admin and the minter. Callable exactly once.
    ///
    /// `minter` is the voucher contract address. Contract instances are
    /// deployed before either is initialised, so the voucher address is known
    /// here; changing it later goes through `propose_minter` and
    /// `accept_minter`.
    pub fn initialize(env: Env, admin: Address, minter: Address) -> Result<(), ReceiptError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(ReceiptError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Minter, &minter);
        common::extend_instance(&env);
        Initialized { admin }.publish(&env);
        Ok(())
    }

    /// Nominates a new minter. Takes effect only once they accept.
    ///
    /// Two steps so a typo in the new address cannot let the wrong contract
    /// mint receipts, or stop receipts entirely. Proposing again replaces the
    /// earlier nominee. `minter` is normally a redeployed voucher contract.
    pub fn propose_minter(
        env: Env,
        admin: Address,
        new_minter: Address,
    ) -> Result<(), ReceiptError> {
        Self::require_admin(&env, &admin)?;
        common::propose_admin(&env, &DataKey::PendingMinter, &new_minter);
        MinterProposed {
            admin,
            pending_minter: new_minter,
        }
        .publish(&env);
        Ok(())
    }

    /// The nominated address accepts and becomes the minter.
    pub fn accept_minter(env: Env, new_minter: Address) -> Result<(), ReceiptError> {
        let new_minter =
            match common::take_pending_admin(&env, &DataKey::PendingMinter, &new_minter) {
                Ok(minter) => minter,
                // The shared helper names the missing nomination after the admin
                // slot; for this role callers expect the minter-specific code.
                Err(common::CommonError::NoPendingAdmin) => {
                    return Err(ReceiptError::NoPendingMinter)
                }
                Err(error) => return Err(error.into()),
            };
        env.storage().instance().set(&DataKey::Minter, &new_minter);
        MinterChanged { minter: new_minter }.publish(&env);
        Ok(())
    }

    /// Records a settled care episode. Callable only by the minter.
    pub fn mint(
        env: Env,
        minter: Address,
        voucher_id: u64,
        beneficiary_ref: BytesN<32>,
        provider: Address,
        service_code: u32,
        amount: i128,
    ) -> Result<(), ReceiptError> {
        let stored: Address = env
            .storage()
            .instance()
            .get(&DataKey::Minter)
            .ok_or(ReceiptError::NotInitialized)?;
        if stored != minter {
            return Err(ReceiptError::NotAuthorized);
        }
        minter.require_auth();

        if amount <= 0 {
            return Err(ReceiptError::InvalidAmount);
        }

        let key = DataKey::Receipt(voucher_id);
        if env.storage().persistent().has(&key) {
            return Err(ReceiptError::ReceiptExists);
        }

        let settled_at = env.ledger().timestamp();
        let receipt = Receipt {
            voucher_id,
            beneficiary_ref: beneficiary_ref.clone(),
            provider: provider.clone(),
            service_code,
            amount,
            settled_at,
        };
        env.storage().persistent().set(&key, &receipt);
        common::extend_persistent(&env, &key);

        let count_key = DataKey::Count(beneficiary_ref.clone());
        let count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);
        env.storage().persistent().set(&count_key, &(count + 1));
        common::extend_persistent(&env, &count_key);

        // Per-provider index, in mint order. A clinic reads its own history
        // from here without an indexer, and two clinics never share a list.
        let provider_count_key = DataKey::ProviderCount(provider.clone());
        let provider_count: u32 = env
            .storage()
            .persistent()
            .get(&provider_count_key)
            .unwrap_or(0);
        let provider_entry_key = DataKey::ProviderReceipt(provider.clone(), provider_count);
        env.storage()
            .persistent()
            .set(&provider_entry_key, &voucher_id);
        common::extend_persistent(&env, &provider_entry_key);
        env.storage()
            .persistent()
            .set(&provider_count_key, &(provider_count + 1));
        common::extend_persistent(&env, &provider_count_key);

        ReceiptMinted {
            beneficiary_ref,
            provider,
            voucher_id,
            service_code,
            amount,
            settled_at,
        }
        .publish(&env);
        Ok(())
    }

    /// Nominates a new admin. Takes effect only once they accept.
    pub fn propose_admin(env: Env, admin: Address, new_admin: Address) -> Result<(), ReceiptError> {
        Self::require_admin(&env, &admin)?;
        common::propose_admin(&env, &DataKey::PendingAdmin, &new_admin);
        AdminProposed {
            admin,
            pending_admin: new_admin,
        }
        .publish(&env);
        Ok(())
    }

    /// The nominated address accepts and becomes admin.
    pub fn accept_admin(env: Env, new_admin: Address) -> Result<(), ReceiptError> {
        let new_admin = common::take_pending_admin(&env, &DataKey::PendingAdmin, &new_admin)?;
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        AdminChanged { new_admin }.publish(&env);
        Ok(())
    }

    pub fn get_pending_admin(env: Env) -> Option<Address> {
        common::read_pending_admin(&env, &DataKey::PendingAdmin)
    }

    pub fn get_pending_minter(env: Env) -> Option<Address> {
        common::read_pending_admin(&env, &DataKey::PendingMinter)
    }

    pub fn get_receipt(env: Env, voucher_id: u64) -> Result<Receipt, ReceiptError> {
        let key = DataKey::Receipt(voucher_id);
        let receipt: Receipt = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(ReceiptError::ReceiptNotFound)?;
        common::extend_persistent(&env, &key);
        Ok(receipt)
    }

    /// How many settled care episodes this beneficiary reference has.
    ///
    /// Extends the counter's TTL like every other persistent read: a patient
    /// who keeps being treated must not have their history archived out from
    /// under them because only the receipt entries were being extended.
    pub fn count_for(env: Env, beneficiary_ref: BytesN<32>) -> u32 {
        let key = DataKey::Count(beneficiary_ref);
        match env.storage().persistent().get::<_, u32>(&key) {
            Some(count) => {
                common::extend_persistent(&env, &key);
                count
            }
            None => 0,
        }
    }

    /// A clinic's receipts in mint order, newest last.
    ///
    /// `start` is the index of the first receipt to return; `limit` is capped
    /// at `MAX_RECEIPTS_PAGE` so one read cannot exceed host limits. Page by
    /// calling again with `start += returned.len()`. A provider with no
    /// receipts returns an empty list, and one clinic's index never includes
    /// another clinic's receipts.
    pub fn receipts_of_provider(
        env: Env,
        provider: Address,
        start: u32,
        limit: u32,
    ) -> Vec<Receipt> {
        let mut out = Vec::new(&env);
        let limit = limit.min(MAX_RECEIPTS_PAGE);
        if limit == 0 {
            return out;
        }

        let count_key = DataKey::ProviderCount(provider.clone());
        let total: u32 = match env.storage().persistent().get(&count_key) {
            Some(total) => total,
            None => return out,
        };
        common::extend_persistent(&env, &count_key);

        if start >= total {
            return out;
        }
        let end = start.saturating_add(limit).min(total);
        for index in start..end {
            let entry_key = DataKey::ProviderReceipt(provider.clone(), index);
            if let Some(voucher_id) = env.storage().persistent().get::<_, u64>(&entry_key) {
                common::extend_persistent(&env, &entry_key);
                // A missing receipt would mean a broken index; skip it rather
                // than failing the whole page.
                if let Ok(receipt) = Self::get_receipt(env.clone(), voucher_id) {
                    out.push_back(receipt);
                }
            }
        }
        out
    }

    /// Human-readable name shown by wallets and explorers.
    ///
    /// Read-only view, no signature required. Receipts are soulbound: there is
    /// no transfer function, so a receipt stays bound to the beneficiary
    /// reference it was minted against.
    pub fn name(env: Env) -> String {
        String::from_str(&env, RECEIPT_NAME)
    }

    /// Token-style symbol shown by wallets and explorers.
    ///
    /// Read-only view, no signature required. Receipts are soulbound.
    pub fn symbol(env: Env) -> String {
        String::from_str(&env, RECEIPT_SYMBOL)
    }

    /// Read-only, per-receipt URI for explorers and wallets.
    ///
    /// The URI is derived only from `voucher_id`. It contains no patient or
    /// beneficiary data — it is an opaque pointer to the receipt, not the
    /// care record itself. No signature required.
    pub fn receipt_uri(env: Env, voucher_id: u64) -> String {
        String::from_str(&env, &alloc::format!("{}{}", RECEIPT_URI_BASE, voucher_id))
    }

    pub fn get_minter(env: Env) -> Result<Address, ReceiptError> {
        env.storage()
            .instance()
            .get(&DataKey::Minter)
            .ok_or(ReceiptError::NotInitialized)
    }

    pub fn get_admin(env: Env) -> Result<Address, ReceiptError> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ReceiptError::NotInitialized)
    }

    /// Crate version this contract was built from, e.g. `"0.2.0"`.
    ///
    /// Lets integrators and the indexer tell deployments apart without
    /// comparing wasm hashes.
    pub fn version(env: Env) -> String {
        String::from_str(&env, env!("CARGO_PKG_VERSION"))
    }

    fn require_admin(env: &Env, admin: &Address) -> Result<(), ReceiptError> {
        let stored: Address =
            common::read_admin(env, &DataKey::Admin).ok_or(ReceiptError::NotInitialized)?;
        common::require_admin(&stored, admin)?;
        Ok(())
    }
}

#[cfg(test)]
mod test;
