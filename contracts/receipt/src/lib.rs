#![no_std]

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

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, Address, BytesN, Env,
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

const DAY_IN_LEDGERS: u32 = 17_280;
const INSTANCE_TTL_THRESHOLD: u32 = 7 * DAY_IN_LEDGERS;
const INSTANCE_TTL_EXTEND: u32 = 30 * DAY_IN_LEDGERS;
const PERSISTENT_TTL_THRESHOLD: u32 = 30 * DAY_IN_LEDGERS;
const PERSISTENT_TTL_EXTEND: u32 = 90 * DAY_IN_LEDGERS;

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
}

#[contract]
pub struct ReceiptBook;

#[contractimpl]
impl ReceiptBook {
    /// Sets the admin and the minter. Callable exactly once.
    ///
    /// `minter` is the voucher contract address. Because the voucher
    /// contract must be deployed before it can be named here, and the
    /// voucher contract needs this address at its own initialization,
    /// deploy order is: receipt, voucher, then `set_minter`.
    pub fn initialize(env: Env, admin: Address, minter: Address) -> Result<(), ReceiptError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(ReceiptError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Minter, &minter);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND);
        Initialized { admin }.publish(&env);
        Ok(())
    }

    /// Repoints the minter at a newly deployed voucher contract. Admin only.
    pub fn set_minter(env: Env, admin: Address, minter: Address) -> Result<(), ReceiptError> {
        Self::require_admin(&env, &admin)?;
        env.storage().instance().set(&DataKey::Minter, &minter);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND);
        MinterChanged { minter }.publish(&env);
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
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_TTL_THRESHOLD,
            PERSISTENT_TTL_EXTEND,
        );

        let count_key = DataKey::Count(beneficiary_ref.clone());
        let count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);
        env.storage().persistent().set(&count_key, &(count + 1));
        env.storage().persistent().extend_ttl(
            &count_key,
            PERSISTENT_TTL_THRESHOLD,
            PERSISTENT_TTL_EXTEND,
        );

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
        env.storage()
            .instance()
            .set(&DataKey::PendingAdmin, &new_admin);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND);
        AdminProposed {
            admin,
            pending_admin: new_admin,
        }
        .publish(&env);
        Ok(())
    }

    /// The nominated address accepts and becomes admin.
    pub fn accept_admin(env: Env, new_admin: Address) -> Result<(), ReceiptError> {
        let pending: Address = env
            .storage()
            .instance()
            .get(&DataKey::PendingAdmin)
            .ok_or(ReceiptError::NoPendingAdmin)?;
        if pending != new_admin {
            return Err(ReceiptError::NotAuthorized);
        }
        new_admin.require_auth();

        env.storage().instance().set(&DataKey::Admin, &new_admin);
        env.storage().instance().remove(&DataKey::PendingAdmin);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND);
        AdminChanged { new_admin }.publish(&env);
        Ok(())
    }

    pub fn get_pending_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::PendingAdmin)
    }

    pub fn get_receipt(env: Env, voucher_id: u64) -> Result<Receipt, ReceiptError> {
        let key = DataKey::Receipt(voucher_id);
        let receipt: Receipt = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(ReceiptError::ReceiptNotFound)?;
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_TTL_THRESHOLD,
            PERSISTENT_TTL_EXTEND,
        );
        Ok(receipt)
    }

    /// How many settled care episodes this beneficiary reference has.
    pub fn count_for(env: Env, beneficiary_ref: BytesN<32>) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::Count(beneficiary_ref))
            .unwrap_or(0)
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

    fn require_admin(env: &Env, admin: &Address) -> Result<(), ReceiptError> {
        let stored: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ReceiptError::NotInitialized)?;
        if stored != *admin {
            return Err(ReceiptError::NotAuthorized);
        }
        admin.require_auth();
        Ok(())
    }
}

#[cfg(test)]
mod test;
