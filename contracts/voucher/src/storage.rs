use soroban_sdk::{Address, Env};

use crate::types::{Config, DataKey, Voucher, VoucherError};

const DAY_IN_LEDGERS: u32 = 17_280;
const INSTANCE_TTL_THRESHOLD: u32 = 7 * DAY_IN_LEDGERS;
const INSTANCE_TTL_EXTEND: u32 = 30 * DAY_IN_LEDGERS;
const PERSISTENT_TTL_THRESHOLD: u32 = 30 * DAY_IN_LEDGERS;
const PERSISTENT_TTL_EXTEND: u32 = 90 * DAY_IN_LEDGERS;

pub(crate) fn extend_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND);
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
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
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
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
    extend_instance(env);
}

pub(crate) fn read_pending_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::PendingAdmin)
}

pub(crate) fn write_pending_admin(env: &Env, pending: &Address) {
    env.storage()
        .instance()
        .set(&DataKey::PendingAdmin, pending);
}

pub(crate) fn clear_pending_admin(env: &Env) {
    env.storage().instance().remove(&DataKey::PendingAdmin);
}
