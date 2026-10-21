use sci_common as common;
use soroban_sdk::{Address, Env};

use crate::DEFAULT_MAX_ATTESTATIONS_PER_DAY;

use crate::types::{Config, DataKey, Voucher, VoucherError};

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
    let voucher: Voucher = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(VoucherError::VoucherNotFound)?;
    common::extend_persistent(env, &key);
    Ok(voucher)
}

/// Writes a voucher and keeps the contract instance alive with it.
///
/// Every state transition goes through here, so a contract that is only
/// ever claimed, attested and settled (never funded) for a month still has
/// its config and id counter extended.
pub(crate) fn write_voucher(env: &Env, voucher: &Voucher) {
    let key = DataKey::Voucher(voucher.id);
    env.storage().persistent().set(&key, voucher);
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
