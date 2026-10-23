use sci_common as common;
use soroban_sdk::{Address, Env, String, Vec};

use crate::types::{DataKey, Provider, ProviderStatus, RegistryError, ServiceItem};

/// Most providers one `list_providers` call may return.
///
/// Caps the response so a registry with a long list cannot make a single
/// read exceed the host's resource limits; callers page with `start`.
pub const MAX_PROVIDERS_PAGE: u32 = 50;

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

/// A clinic's declared voucher capacity; 0 (unlimited) when never set.
pub(crate) fn read_capacity(env: &Env, addr: &Address) -> u32 {
    let key = DataKey::Capacity(addr.clone());
    match env.storage().persistent().get(&key) {
        Some(capacity) => {
            common::extend_persistent(env, &key);
            capacity
        }
        None => 0,
    }
}

pub(crate) fn write_capacity(env: &Env, addr: &Address, capacity: u32) {
    let key = DataKey::Capacity(addr.clone());
    env.storage().persistent().set(&key, &capacity);
    common::extend_persistent(env, &key);
}

pub(crate) fn write_attester(env: &Env, addr: &Address, enabled: bool) {
    let key = DataKey::Attester(addr.clone());
    env.storage().persistent().set(&key, &enabled);
    common::extend_persistent(env, &key);
}

/// Total number of registered providers.
pub(crate) fn read_provider_count(env: &Env) -> u32 {
    match env
        .storage()
        .persistent()
        .get::<_, u32>(&DataKey::ProviderCount)
    {
        Some(count) => {
            common::extend_persistent(env, &DataKey::ProviderCount);
            count
        }
        None => 0,
    }
}

/// Writes the total number of registered providers, extending its TTL.
pub(crate) fn write_provider_count(env: &Env, count: &u32) {
    env.storage()
        .persistent()
        .set(&DataKey::ProviderCount, count);
    common::extend_persistent(env, &DataKey::ProviderCount);
}

/// Number of active providers in a country.
pub(crate) fn read_country_count(env: &Env, country: &String) -> u32 {
    match env
        .storage()
        .persistent()
        .get::<_, u32>(&DataKey::CountryCount(country.clone()))
    {
        Some(count) => {
            common::extend_persistent(env, &DataKey::CountryCount(country.clone()));
            count
        }
        None => 0,
    }
}

/// Writes the number of active providers in a country, extending its TTL.
pub(crate) fn write_country_count(env: &Env, country: &String, count: &u32) {
    env.storage()
        .persistent()
        .set(&DataKey::CountryCount(country.clone()), count);
    common::extend_persistent(env, &DataKey::CountryCount(country.clone()));
}

/// Writes the provider address at `index` in registration order.
pub(crate) fn write_provider_at(env: &Env, index: u32, addr: &Address) {
    let key = DataKey::ProviderAt(index);
    env.storage().persistent().set(&key, addr);
    common::extend_persistent(env, &key);
}

/// Reads the provider address at `index`, if any.
pub(crate) fn read_provider_at(env: &Env, index: u32) -> Option<Address> {
    let key = DataKey::ProviderAt(index);
    match env.storage().persistent().get::<_, Address>(&key) {
        Some(addr) => {
            common::extend_persistent(env, &key);
            Some(addr)
        }
        None => None,
    }
}

/// Address of the active provider at `index` in a country's list.
pub(crate) fn read_country_at(env: &Env, country: &String, index: u32) -> Option<Address> {
    let key = DataKey::CountryAt(country.clone(), index);
    match env.storage().persistent().get::<_, Address>(&key) {
        Some(addr) => {
            common::extend_persistent(env, &key);
            Some(addr)
        }
        None => None,
    }
}

/// Writes the provider address at `index` in a country's list, extending its TTL.
pub(crate) fn write_country_at(env: &Env, country: &String, index: u32, addr: &Address) {
    let key = DataKey::CountryAt(country.clone(), index);
    env.storage().persistent().set(&key, addr);
    common::extend_persistent(env, &key);
}

/// Providers in registration order, paged by `start` and `limit`.
///
/// `limit` is capped at `MAX_PROVIDERS_PAGE` so a single read cannot exceed
/// host limits. Page by calling again with `start += returned.len()`. An
/// empty registry or a `start` beyond the end returns an empty list.
pub(crate) fn list_providers(env: &Env, start: u32, limit: u32) -> Vec<Provider> {
    use soroban_sdk::Vec;

    let mut out = Vec::new(env);
    let limit = limit.min(MAX_PROVIDERS_PAGE);
    if limit == 0 {
        return out;
    }

    let total = read_provider_count(env);
    if start >= total {
        return out;
    }
    let end = start.saturating_add(limit).min(total);

    for index in start..end {
        if let Some(addr) = read_provider_at(env, index) {
            if let Ok(provider) = read_provider(env, &addr) {
                out.push_back(provider);
            }
        }
    }
    out
}

/// Providers in a given country, paged by `start` and `limit`.
///
/// `limit` is capped at `MAX_PROVIDERS_PAGE` so a single read cannot exceed
/// host limits. Page by calling again with `start += returned.len()`. An
/// empty registry or a `start` beyond the end returns an empty list. No
/// signature is required; this is the on-chain discovery any app or auditor
/// can use instead of trusting an indexer.
pub(crate) fn list_providers_by_country(
    env: &Env,
    country: &String,
    start: u32,
    limit: u32,
) -> Vec<Provider> {
    use soroban_sdk::Vec;

    let mut out = Vec::new(env);
    let limit = limit.min(MAX_PROVIDERS_PAGE);
    if limit == 0 {
        return out;
    }

    let total = read_country_count(env, country);
    if start >= total {
        return out;
    }
    let end = start.saturating_add(limit).min(total);

    for index in start..end {
        if let Some(addr) = read_country_at(env, country, index) {
            if let Ok(provider) = read_provider(env, &addr) {
                // Only return active providers
                if provider.status == ProviderStatus::Active {
                    out.push_back(provider);
                }
            }
        }
    }
    out
}
