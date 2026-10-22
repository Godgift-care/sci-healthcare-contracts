use sci_common as common;
use soroban_sdk::{Address, Env};

use crate::types::{DataKey, Provider, RegistryError, ServiceItem};

pub(crate) fn extend_instance(env: &Env) {
    common::extend_instance(env);
}

pub(crate) fn read_admin(env: &Env) -> Result<Address, RegistryError> {
    common::read_admin(env, &DataKey::Admin).ok_or(RegistryError::NotInitialized)
}

pub(crate) fn write_admin(env: &Env, admin: &Address) {
    common::write_admin(env, &DataKey::Admin, admin);
}

pub(crate) fn has_admin(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Admin)
}

pub(crate) fn read_pending_admin(env: &Env) -> Option<Address> {
    common::read_pending_admin(env, &DataKey::PendingAdmin)
}

pub(crate) fn read_provider(env: &Env, addr: &Address) -> Result<Provider, RegistryError> {
    let key = DataKey::Provider(addr.clone());
    let provider: Provider = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(RegistryError::ProviderNotFound)?;
    common::extend_persistent(env, &key);
    Ok(provider)
}

pub(crate) fn write_provider(env: &Env, addr: &Address, provider: &Provider) {
    let key = DataKey::Provider(addr.clone());
    env.storage().persistent().set(&key, provider);
    common::extend_persistent(env, &key);
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
    common::extend_persistent(env, &key);
    let mut item = item;
    item.apply_due_price(env.ledger().timestamp());
    Ok(item)
}

pub(crate) fn write_service(env: &Env, provider: &Address, code: u32, item: &ServiceItem) {
    let key = DataKey::Service(provider.clone(), code);
    env.storage().persistent().set(&key, item);
    common::extend_persistent(env, &key);
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
            common::extend_persistent(env, &key);
            enabled
        }
        None => false,
    }
}

pub(crate) fn write_attester(env: &Env, addr: &Address, enabled: bool) {
    let key = DataKey::Attester(addr.clone());
    env.storage().persistent().set(&key, &enabled);
    common::extend_persistent(env, &key);
}

/// Whether `addr` currently holds verifier rights.
///
/// Extends the entry on every read, like providers, services and attesters, so
/// a verifier who keeps checking clinic credentials never has their role
/// archived out from under them.
pub(crate) fn read_verifier(env: &Env, addr: &Address) -> bool {
    let key = DataKey::Verifier(addr.clone());
    match env.storage().persistent().get::<_, bool>(&key) {
        Some(enabled) => {
            common::extend_persistent(env, &key);
            enabled
        }
        None => false,
    }
}

pub(crate) fn write_verifier(env: &Env, addr: &Address, enabled: bool) {
    let key = DataKey::Verifier(addr.clone());
    env.storage().persistent().set(&key, &enabled);
    common::extend_persistent(env, &key);
}
