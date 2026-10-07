use soroban_sdk::{Address, Env};

use crate::types::{DataKey, Provider, RegistryError, ServiceItem};

/// Roughly one day of ledgers, assuming ~5 second close times.
pub(crate) const DAY_IN_LEDGERS: u32 = 17_280;
pub(crate) const INSTANCE_TTL_THRESHOLD: u32 = 7 * DAY_IN_LEDGERS;
pub(crate) const INSTANCE_TTL_EXTEND: u32 = 30 * DAY_IN_LEDGERS;
pub(crate) const PERSISTENT_TTL_THRESHOLD: u32 = 30 * DAY_IN_LEDGERS;
pub(crate) const PERSISTENT_TTL_EXTEND: u32 = 90 * DAY_IN_LEDGERS;

pub(crate) fn extend_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND);
}

pub(crate) fn read_admin(env: &Env) -> Result<Address, RegistryError> {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(RegistryError::NotInitialized)
}

pub(crate) fn write_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

pub(crate) fn has_admin(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Admin)
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

pub(crate) fn read_provider(env: &Env, addr: &Address) -> Result<Provider, RegistryError> {
    let key = DataKey::Provider(addr.clone());
    let provider: Provider = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(RegistryError::ProviderNotFound)?;
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
    Ok(provider)
}

pub(crate) fn write_provider(env: &Env, addr: &Address, provider: &Provider) {
    let key = DataKey::Provider(addr.clone());
    env.storage().persistent().set(&key, provider);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
}

pub(crate) fn has_provider(env: &Env, addr: &Address) -> bool {
    env.storage()
        .persistent()
        .has(&DataKey::Provider(addr.clone()))
}

pub(crate) fn read_service(
    env: &Env,
    provider: &Address,
    code: u32,
) -> Result<ServiceItem, RegistryError> {
    let key = DataKey::Service(provider.clone(), code);
    let item: ServiceItem = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(RegistryError::ServiceNotFound)?;
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
    Ok(item)
}

pub(crate) fn write_service(env: &Env, provider: &Address, code: u32, item: &ServiceItem) {
    let key = DataKey::Service(provider.clone(), code);
    env.storage().persistent().set(&key, item);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
}

pub(crate) fn remove_service(env: &Env, provider: &Address, code: u32) {
    env.storage()
        .persistent()
        .remove(&DataKey::Service(provider.clone(), code));
}

/// Whether `addr` currently holds attester rights.
///
/// Extends the entry on every read, like providers and services, so an
/// attester who keeps confirming care never has their role archived out
/// from under them.
pub(crate) fn read_attester(env: &Env, addr: &Address) -> bool {
    let key = DataKey::Attester(addr.clone());
    match env.storage().persistent().get::<_, bool>(&key) {
        Some(enabled) => {
            env.storage().persistent().extend_ttl(
                &key,
                PERSISTENT_TTL_THRESHOLD,
                PERSISTENT_TTL_EXTEND,
            );
            enabled
        }
        None => false,
    }
}

pub(crate) fn write_attester(env: &Env, addr: &Address, enabled: bool) {
    let key = DataKey::Attester(addr.clone());
    env.storage().persistent().set(&key, &enabled);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
}
