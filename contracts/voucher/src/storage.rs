use sci_common as common;
use soroban_sdk::{contracttype, Address, BytesN, Env};

use crate::{DEFAULT_DISPUTE_WINDOW_BOUNDS, DEFAULT_MAX_ATTESTATIONS_PER_DAY};

use crate::types::{
    Config, DataKey, Exposure, Voucher, VoucherError, VoucherOptions, VoucherStatus,
};

/// How a voucher is laid out in storage.
///
/// `Voucher` is a named struct, which the host encodes as an XDR map: every
/// field is stored with its name as a symbol key, so twelve field names ride
/// along in every voucher entry and are paid for in rent for its whole life.
/// This tuple struct is encoded as an XDR vec — values only, in order — and
/// leaves out the id, which is already in the entry's key. The public API
/// still takes and returns `Voucher`; only the bytes at rest change.
///
/// Field order is part of the storage format: never reorder, only append.
#[contracttype(export = false)]
#[derive(Clone)]
pub(crate) struct StoredVoucher(
    Address,         // funder
    BytesN<32>,      // beneficiary_ref
    Address,         // provider
    u32,             // service_code
    i128,            // amount
    VoucherStatus,   // status
    u64,             // created_at
    u64,             // expires_at
    u64,             // claimed_at
    u64,             // attested_at
    u64,             // dispute_deadline
    Option<Address>, // attester
);

impl StoredVoucher {
    pub(crate) fn from_voucher(v: &Voucher) -> Self {
        StoredVoucher(
            v.funder.clone(),
            v.beneficiary_ref.clone(),
            v.provider.clone(),
            v.service_code,
            v.amount,
            v.status,
            v.created_at,
            v.expires_at,
            v.claimed_at,
            v.attested_at,
            v.dispute_deadline,
            v.attester.clone(),
        )
    }

    pub(crate) fn into_voucher(self, id: u64) -> Voucher {
        Voucher {
            id,
            funder: self.0,
            beneficiary_ref: self.1,
            provider: self.2,
            service_code: self.3,
            amount: self.4,
            status: self.5,
            created_at: self.6,
            expires_at: self.7,
            claimed_at: self.8,
            attested_at: self.9,
            dispute_deadline: self.10,
            attester: self.11,
        }
    }
}

pub(crate) fn extend_instance(env: &Env) {
    common::extend_instance(env);
}

pub(crate) fn has_config(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Config)
}

pub(crate) fn read_config(env: &Env) -> Result<Config, VoucherError> {
    env.storage()
        .instance()
        .get(&DataKey::Config)
        .ok_or(VoucherError::NotInitialized)
}

pub(crate) fn write_config(env: &Env, config: &Config) {
    env.storage().instance().set(&DataKey::Config, config);
}

/// Returns the next voucher id and advances the counter.
pub(crate) fn bump_next_id(env: &Env) -> u64 {
    let id: u64 = env.storage().instance().get(&DataKey::NextId).unwrap_or(1);
    env.storage().instance().set(&DataKey::NextId, &(id + 1));
    id
}

pub(crate) fn peek_next_id(env: &Env) -> u64 {
    env.storage().instance().get(&DataKey::NextId).unwrap_or(1)
}

pub(crate) fn read_voucher(env: &Env, id: u64) -> Result<Voucher, VoucherError> {
    let key = DataKey::Voucher(id);
    let stored: StoredVoucher = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(VoucherError::VoucherNotFound)?;
    common::extend_persistent(env, &key);
    Ok(stored.into_voucher(id))
}

/// Writes a voucher and keeps the contract instance alive with it.
///
/// Every state transition goes through here, so a contract that is only
/// ever claimed, attested and settled (never funded) for a month still has
/// its config and id counter extended.
pub(crate) fn write_voucher(env: &Env, voucher: &Voucher) {
    let key = DataKey::Voucher(voucher.id);
    env.storage()
        .persistent()
        .set(&key, &StoredVoucher::from_voucher(voucher));
    common::extend_persistent(env, &key);
    extend_instance(env);
}

/// Total amount currently held in escrow; 0 if nothing has ever been funded.
pub(crate) fn read_open_escrow(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::OpenEscrow)
        .unwrap_or(0)
}

fn write_open_escrow(env: &Env, total: i128) {
    env.storage().instance().set(&DataKey::OpenEscrow, &total);
    extend_instance(env);
}

/// Adds newly escrowed funds to the running total.
pub(crate) fn increase_open_escrow(env: &Env, amount: i128) -> Result<(), VoucherError> {
    let total = read_open_escrow(env)
        .checked_add(amount)
        .ok_or(VoucherError::MathOverflow)?;
    write_open_escrow(env, total);
    Ok(())
}

/// Removes funds that have left escrow, either to the provider or the funder.
pub(crate) fn decrease_open_escrow(env: &Env, amount: i128) -> Result<(), VoucherError> {
    let total = read_open_escrow(env)
        .checked_sub(amount)
        .ok_or(VoucherError::MathOverflow)?;
    write_open_escrow(env, total);
    Ok(())
}

pub(crate) fn read_attestation_cap(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::AttestationCap)
        .unwrap_or(DEFAULT_MAX_ATTESTATIONS_PER_DAY)
}

pub(crate) fn write_attestation_cap(env: &Env, cap: u32) {
    env.storage().instance().set(&DataKey::AttestationCap, &cap);
}

/// Attestations `attester` has made on UTC `day`; 0 if none.
pub(crate) fn read_attestations(env: &Env, attester: &Address, day: u64) -> u32 {
    env.storage()
        .temporary()
        .get(&DataKey::Attestations(attester.clone(), day))
        .unwrap_or(0)
}

pub(crate) fn write_attestations(env: &Env, attester: &Address, day: u64, count: u32) {
    let key = DataKey::Attestations(attester.clone(), day);
    env.storage().temporary().set(&key, &count);
    env.storage()
        .temporary()
        .extend_ttl(&key, common::ATTESTATIONS_TTL, common::ATTESTATIONS_TTL);
}

/// A voucher's options; all off when it was funded without any.
///
/// The entry lives and dies with the voucher, so it is extended whenever it
/// is read, exactly like the voucher itself.
pub(crate) fn read_options(env: &Env, id: u64) -> VoucherOptions {
    let key = DataKey::Options(id);
    match env.storage().persistent().get(&key) {
        Some(options) => {
            common::extend_persistent(env, &key);
            options
        }
        None => VoucherOptions::default(),
    }
}

/// Stores a voucher's options — only when one is set, so the common voucher
/// gets no second entry.
pub(crate) fn write_options(env: &Env, id: u64, options: &VoucherOptions) {
    if *options == VoucherOptions::default() {
        return;
    }
    let key = DataKey::Options(id);
    env.storage().persistent().set(&key, options);
    common::extend_persistent(env, &key);
}

pub(crate) fn read_dispute_window_bounds(env: &Env) -> (u64, u64) {
    env.storage()
        .instance()
        .get(&DataKey::DisputeWindowBounds)
        .unwrap_or(DEFAULT_DISPUTE_WINDOW_BOUNDS)
}

pub(crate) fn write_dispute_window_bounds(env: &Env, bounds: (u64, u64)) {
    env.storage()
        .instance()
        .set(&DataKey::DisputeWindowBounds, &bounds);
}

/// What `provider` currently holds in escrow; empty if nothing.
pub(crate) fn read_exposure(env: &Env, provider: &Address) -> Exposure {
    let key = DataKey::Exposure(provider.clone());
    match env.storage().persistent().get(&key) {
        Some(exposure) => {
            common::extend_persistent(env, &key);
            exposure
        }
        None => Exposure::default(),
    }
}

pub(crate) fn write_exposure(env: &Env, provider: &Address, exposure: &Exposure) {
    let key = DataKey::Exposure(provider.clone());
    env.storage().persistent().set(&key, exposure);
    common::extend_persistent(env, &key);
}

/// The per-clinic escrow cap; 0 (unlimited) when never set.
pub(crate) fn read_escrow_cap(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::EscrowCap)
        .unwrap_or(0)
}

pub(crate) fn write_escrow_cap(env: &Env, cap: i128) {
    env.storage().instance().set(&DataKey::EscrowCap, &cap);
}

#[cfg(test)]
mod storage_coverage {
    use super::*;

    #[test]
    fn write_options_with_default_options_returns_early() {
        let env = Env::default();
        let contract_id = env.register(crate::VoucherEscrow, ());
        env.as_contract(&contract_id, || {
            let id = 1u64;
            let options = VoucherOptions::default();
            write_options(&env, id, &options);
            assert_eq!(read_options(&env, id), VoucherOptions::default());
        });
    }
}
