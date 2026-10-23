#![no_std]
// A panic aborts the whole invocation with an opaque host error. Contract
// code must return a typed error instead, so both are denied outside tests.
// Tests keep them for brevity: they are fail-fast assertions, not contract
// behaviour.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

//! # SCI Healthcare — Provider Registry
//!
//! Answers one question for the rest of the protocol: *who is allowed to do
//! what*. It holds verified care providers, the coarse service categories
//! each one is approved to bill for, and the set of attesters permitted to
//! confirm that care was delivered.
//!
//! No patient-identifying data is stored here, or anywhere else in this
//! protocol. Services are coarse categories, never diagnoses.

mod events;
mod storage;
mod types;

#[cfg(test)]
mod test;

use sci_common as common;
use soroban_sdk::{contract, contractimpl, Address, Env, String, Vec};

pub use events::{
    AdminChanged, AdminProposed, AttesterAdded, AttesterRemoved, CapacityChanged, Initialized,
    ProviderRegistered, ProviderStatusChanged, ServiceAvailabilityChanged, ServicePriceScheduled,
    ServiceRemoved, ServiceUpserted,
};
pub use types::{DataKey, Provider, ProviderStatus, RegistryError, ServiceItem};

use storage::{
    extend_instance, has_admin, has_provider, list_providers_by_country, read_admin, read_attester,
    read_capacity, read_pending_admin, read_provider, read_service,
    remove_service as storage_remove_service, write_admin, write_attester, write_capacity,
    write_provider, write_service,
};

/// Least notice a clinic must give before a scheduled price takes effect.
/// 24 hours, so a funder who has just seen a price can still pay it.
pub const MIN_PRICE_NOTICE_SECS: u64 = 24 * 60 * 60;

/// Most providers one `list_providers` call may return.
pub use storage::MAX_PROVIDERS_PAGE;

#[contract]
pub struct Registry;

#[contractimpl]
impl Registry {
    /// Sets the administrator. Callable exactly once.
    pub fn initialize(env: Env, admin: Address) -> Result<(), RegistryError> {
        if has_admin(&env) {
            return Err(RegistryError::AlreadyInitialized);
        }
        write_admin(&env, &admin);
        extend_instance(&env);
        Initialized { admin }.publish(&env);
        Ok(())
    }

    /// Self-registers a provider in `Pending` state.
    ///
    /// Registration is permissionless; activation is not. A provider cannot
    /// receive vouchers until an admin moves it to `Active`.
    pub fn register_provider(
        env: Env,
        owner: Address,
        name: String,
        country: String,
    ) -> Result<(), RegistryError> {
        owner.require_auth();

        if name.is_empty() {
            return Err(RegistryError::EmptyName);
        }
        // ISO 3166-1 alpha-2 is always two characters.
        if country.len() != 2 {
            return Err(RegistryError::InvalidCountry);
        }
        if has_provider(&env, &owner) {
            return Err(RegistryError::ProviderExists);
        }

        let registered_at = env.ledger().timestamp();
        let provider = Provider {
            owner: owner.clone(),
            name: name.clone(),
            country: country.clone(),
            status: ProviderStatus::Pending,
            registered_at,
        };
        write_provider(&env, &owner, &provider);
        extend_instance(&env);

        // Append to the registration index so any app can enumerate clinics
        // without an indexer, in registration order.
        let count = storage::read_provider_count(&env);
        storage::write_provider_at(&env, count, &owner);
        storage::write_provider_count(&env, &(count + 1));

        // `CountryCount` for this country is left unset; `read_country_count`
        // returns 0 when absent, so `providers_in` works even if the provider
        // never becomes active. The entry is created when the provider is
        // activated in `set_provider_status`.

        ProviderRegistered {
            provider: owner,
            name,
            country,
            registered_at,
        }
        .publish(&env);
        Ok(())
    }

    /// Moves a provider between lifecycle states. Admin only.
    ///
    /// When a provider becomes `Active`, they are added to the per-country
    /// index so `providers_in` can find them. When they become non-`Active`
    /// (e.g. `Suspended`), they are removed from the index using swap-and-pop
    /// so the index stays consistent and reads remain O(1) amortized.
    pub fn set_provider_status(
        env: Env,
        admin: Address,
        provider_addr: Address,
        status: ProviderStatus,
    ) -> Result<(), RegistryError> {
        Self::require_admin(&env, &admin)?;

        let mut provider = read_provider(&env, &provider_addr)?;
        let prev_status = provider.status;
        provider.status = status;
        write_provider(&env, &provider_addr, &provider);
        extend_instance(&env);

        // Update the per-country index when crossing the Active boundary.
        if prev_status != status {
            let country = &provider.country;
            match (prev_status, status) {
                // Provider just became active: add to country index.
                (_, ProviderStatus::Active) => {
                    let count = storage::read_country_count(&env, country);
                    storage::write_country_at(&env, country, count, &provider_addr);
                    storage::write_country_count(&env, country, &(count + 1));
                }
                // Provider just became non-active: remove from country index
                // using swap-and-pop.
                (ProviderStatus::Active, _) => {
                    // Find the provider's index in the country list.
                    let total = storage::read_country_count(&env, country);
                    let mut idx = None;
                    for i in 0..total {
                        if let Some(addr) = storage::read_country_at(&env, country, i) {
                            if addr == provider_addr {
                                idx = Some(i);
                                break;
                            }
                        }
                    }
                    if let Some(i) = idx {
                        // Swap with the last element and pop.
                        let last_idx = total.saturating_sub(1);
                        if i != last_idx {
                            if let Some(last_addr) =
                                storage::read_country_at(&env, country, last_idx)
                            {
                                storage::write_country_at(&env, country, i, &last_addr);
                            }
                        }
                        storage::write_country_count(&env, country, &(total - 1));
                    }
                }
                // Reactivation (Pending -> Active or Suspended -> Active) is
                // handled above; no double-add because we only act on the
                // transition from non-Active to Active.
                _ => {}
            }
        }

        ProviderStatusChanged {
            provider: provider_addr,
            status,
        }
        .publish(&env);
        Ok(())
    }

    /// Creates or updates a service the provider bills for.
    ///
    /// Only an `Active` provider may maintain a catalog, so a suspended
    /// clinic cannot quietly re-price itself back into circulation.
    ///
    /// A new service is listed at `price` straight away, and so is a lower
    /// price for an existing one. Raising an existing price is rejected with
    /// `PriceIncreaseNeedsNotice`: use `schedule_service_price`, so a funder
    /// never sees one price and finds another minutes later. A price already
    /// scheduled is left in place.
    pub fn upsert_service(
        env: Env,
        provider_addr: Address,
        code: u32,
        label: String,
        price: i128,
    ) -> Result<(), RegistryError> {
        provider_addr.require_auth();

        if price <= 0 {
            return Err(RegistryError::InvalidPrice);
        }
        if label.is_empty() {
            return Err(RegistryError::EmptyName);
        }

        let provider = read_provider(&env, &provider_addr)?;
        if provider.status != ProviderStatus::Active {
            return Err(RegistryError::ProviderNotActive);
        }

        let item = match read_service(&env, &provider_addr, code) {
            Ok(existing) => {
                if price > existing.price {
                    return Err(RegistryError::PriceIncreaseNeedsNotice);
                }
                // Re-pricing or relabelling a paused service must not quietly
                // put it back on sale; only `set_service_available` does that.
                ServiceItem {
                    label: label.clone(),
                    price,
                    active: true,
                    ..existing
                }
            }
            Err(_) => ServiceItem {
                code,
                label: label.clone(),
                price,
                active: true,
                available: true,
                pending_price: 0,
                effective_at: 0,
            },
        };
        write_service(&env, &provider_addr, code, &item);
        extend_instance(&env);

        ServiceUpserted {
            provider: provider_addr,
            code,
            label,
            price,
        }
        .publish(&env);
        Ok(())
    }

    /// Schedules a new price for a listed service, effective at `effective_at`.
    ///
    /// The time must be at least `MIN_PRICE_NOTICE_SECS` away, so apps can
    /// warn funders before it lands. Until then `get_service_price` keeps
    /// returning the current price; from then on it returns this one.
    /// Scheduling again replaces the earlier schedule, and scheduling the
    /// current price is how a clinic calls one off.
    pub fn schedule_service_price(
        env: Env,
        provider_addr: Address,
        code: u32,
        price: i128,
        effective_at: u64,
    ) -> Result<(), RegistryError> {
        provider_addr.require_auth();

        if price <= 0 {
            return Err(RegistryError::InvalidPrice);
        }
        let provider = read_provider(&env, &provider_addr)?;
        if provider.status != ProviderStatus::Active {
            return Err(RegistryError::ProviderNotActive);
        }
        let mut item = read_service(&env, &provider_addr, code)?;

        let earliest = env
            .ledger()
            .timestamp()
            .saturating_add(MIN_PRICE_NOTICE_SECS);
        if effective_at < earliest {
            return Err(RegistryError::PriceNoticeTooShort);
        }

        item.pending_price = price;
        item.effective_at = effective_at;
        write_service(&env, &provider_addr, code, &item);
        extend_instance(&env);

        ServicePriceScheduled {
            provider: provider_addr,
            code,
            price,
            effective_at,
        }
        .publish(&env);
        Ok(())
    }

    /// Removes a service from a provider's catalog.
    ///
    /// Vouchers already funded against this code are unaffected — they
    /// carry their own agreed amount and settle normally.
    pub fn remove_service(
        env: Env,
        provider_addr: Address,
        code: u32,
    ) -> Result<(), RegistryError> {
        provider_addr.require_auth();

        // Confirms the service exists before removing it.
        read_service(&env, &provider_addr, code)?;
        storage_remove_service(&env, &provider_addr, code);
        extend_instance(&env);

        ServiceRemoved {
            provider: provider_addr,
            code,
        }
        .publish(&env);
        Ok(())
    }

    /// Pauses or resumes a service without delisting it.
    ///
    /// While paused, `voucher` refuses to fund the service. Vouchers already
    /// funded against it are unaffected and settle normally.
    pub fn set_service_available(
        env: Env,
        provider_addr: Address,
        code: u32,
        available: bool,
    ) -> Result<(), RegistryError> {
        provider_addr.require_auth();

        let mut item = read_service(&env, &provider_addr, code)?;
        item.available = available;
        write_service(&env, &provider_addr, code, &item);
        extend_instance(&env);

        ServiceAvailabilityChanged {
            provider: provider_addr,
            code,
            available,
        }
        .publish(&env);
        Ok(())
    }

    /// Declares how many vouchers this clinic can have outstanding (funded but
    /// not yet settled or refunded) at once. `0` means unlimited, which is
    /// also the default for a clinic that never sets it.
    ///
    /// `voucher` refuses to fund a new voucher at a clinic already at its
    /// capacity (`ProviderAtCapacity`), so funders' money is not stuck at a
    /// clinic that cannot see the patients before the vouchers expire.
    /// Lowering the capacity below the current count affects only new
    /// vouchers; outstanding ones settle or refund as normal.
    pub fn set_capacity(
        env: Env,
        provider_addr: Address,
        max_outstanding: u32,
    ) -> Result<(), RegistryError> {
        provider_addr.require_auth();

        read_provider(&env, &provider_addr)?;
        write_capacity(&env, &provider_addr, max_outstanding);
        extend_instance(&env);

        CapacityChanged {
            provider: provider_addr,
            max_outstanding,
        }
        .publish(&env);
        Ok(())
    }

    /// Grants attester rights. Admin only.
    pub fn add_attester(env: Env, admin: Address, attester: Address) -> Result<(), RegistryError> {
        Self::require_admin(&env, &admin)?;
        write_attester(&env, &attester, true);
        extend_instance(&env);
        AttesterAdded { attester }.publish(&env);
        Ok(())
    }

    /// Revokes attester rights. Admin only.
    pub fn remove_attester(
        env: Env,
        admin: Address,
        attester: Address,
    ) -> Result<(), RegistryError> {
        Self::require_admin(&env, &admin)?;
        write_attester(&env, &attester, false);
        extend_instance(&env);
        AttesterRemoved { attester }.publish(&env);
        Ok(())
    }

    /// Nominates a new admin. Takes effect only once they accept.
    ///
    /// Two steps so a typo in the new address cannot permanently strand
    /// provider verification. Proposing again replaces the earlier nominee.
    pub fn propose_admin(
        env: Env,
        admin: Address,
        new_admin: Address,
    ) -> Result<(), RegistryError> {
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
    pub fn accept_admin(env: Env, new_admin: Address) -> Result<(), RegistryError> {
        let new_admin = common::take_pending_admin(&env, &DataKey::PendingAdmin, &new_admin)?;
        write_admin(&env, &new_admin);
        AdminChanged { new_admin }.publish(&env);
        Ok(())
    }

    // ----- views -----

    pub fn get_admin(env: Env) -> Result<Address, RegistryError> {
        read_admin(&env)
    }

    pub fn get_pending_admin(env: Env) -> Option<Address> {
        read_pending_admin(&env)
    }

    pub fn get_provider(env: Env, provider_addr: Address) -> Result<Provider, RegistryError> {
        read_provider(&env, &provider_addr)
    }

    /// Providers in registration order, paged by `start` and `limit`.
    ///
    /// `start` is the index of the first provider to return; `limit` is capped
    /// at `MAX_PROVIDERS_PAGE` so one read cannot exceed host limits. Page by
    /// calling again with `start += returned.len()`. An empty registry or a
    /// `start` beyond the end returns an empty list. No signature is required;
    /// this is the on-chain discovery any app or auditor can use instead of
    /// trusting an indexer.
    pub fn list_providers(env: Env, start: u32, limit: u32) -> Vec<Provider> {
        storage::list_providers(&env, start, limit)
    }

    /// The check `voucher` relies on before escrowing any funds.
    pub fn is_active_provider(env: Env, provider_addr: Address) -> bool {
        match read_provider(&env, &provider_addr) {
            Ok(p) => p.status == ProviderStatus::Active,
            Err(_) => false,
        }
    }

    /// The country a provider is registered in, as an ISO 3166-1 alpha-2
    /// code. Empty when the provider is not registered.
    ///
    /// The check `sponsor` relies on before matching a voucher for a clinic
    /// in one of its countries. Declared over primitives so the two
    /// contracts share no types.
    pub fn get_provider_country(env: Env, provider_addr: Address) -> String {
        match read_provider(&env, &provider_addr) {
            Ok(p) => p.country,
            Err(_) => String::from_str(&env, ""),
        }
    }

    pub fn get_service(
        env: Env,
        provider_addr: Address,
        code: u32,
    ) -> Result<ServiceItem, RegistryError> {
        read_service(&env, &provider_addr, code)
    }

    /// Returns the subset of `codes` that this provider actively bills for.
    ///
    /// Batched so a client can price a whole catalog in one RPC round trip
    /// instead of one call per service.
    pub fn get_services(env: Env, provider_addr: Address, codes: Vec<u32>) -> Vec<ServiceItem> {
        let mut out = Vec::new(&env);
        for code in codes.iter() {
            if let Ok(item) = read_service(&env, &provider_addr, code) {
                if item.active {
                    out.push_back(item);
                }
            }
        }
        out
    }

    /// Price for a provider's service, or 0 if absent or inactive.
    ///
    /// A scheduled price is returned from its `effective_at` onwards.
    ///
    /// A paused service still reports its price, so `voucher` can tell
    /// "not offered" apart from "temporarily unavailable".
    ///
    /// Returns a primitive rather than a struct so the voucher contract can
    /// validate pricing over a cross-contract interface without having to
    /// redeclare this contract's types.
    pub fn get_service_price(env: Env, provider_addr: Address, code: u32) -> i128 {
        match read_service(&env, &provider_addr, code) {
            Ok(item) if item.active => item.price,
            _ => 0,
        }
    }

    /// The scheduled price and when it takes effect, or `(0, 0)` if no
    /// change is pending. Lets apps warn funders about an upcoming price.
    pub fn get_upcoming_price(
        env: Env,
        provider_addr: Address,
        code: u32,
    ) -> Result<(i128, u64), RegistryError> {
        let item = read_service(&env, &provider_addr, code)?;
        Ok((item.pending_price, item.effective_at))
    }

    /// Whether the service is listed, active and not paused — the second
    /// check `voucher` makes before escrowing, after the price.
    pub fn is_service_available(env: Env, provider_addr: Address, code: u32) -> bool {
        match read_service(&env, &provider_addr, code) {
            Ok(item) => item.active && item.available,
            Err(_) => false,
        }
    }

    pub fn is_attester(env: Env, addr: Address) -> bool {
        read_attester(&env, &addr)
    }

    /// Most vouchers the clinic accepts outstanding at once; `0` is
    /// unlimited. `voucher` reads this before funding.
    pub fn get_capacity(env: Env, provider_addr: Address) -> u32 {
        read_capacity(&env, &provider_addr)
    }

    /// The subset of providers in a given country, paged by `start` and `limit`.
    ///
    /// Only `Active` providers are returned. `limit` is capped at
    /// `MAX_PROVIDERS_PAGE` so one read cannot exceed host limits. Page by
    /// calling again with `start += returned.len()`. An empty registry or a
    /// `start` beyond the end returns an empty list. No signature is required;
    /// this is the on-chain discovery any app or auditor can use instead of
    /// trusting an indexer.
    pub fn providers_in(env: Env, country: String, start: u32, limit: u32) -> Vec<Provider> {
        list_providers_by_country(&env, &country, start, limit)
    }

    /// Crate version this contract was built from, e.g. `"0.2.0"`.
    ///
    /// Lets integrators and the indexer tell deployments apart without
    /// comparing wasm hashes.
    pub fn version(env: Env) -> String {
        String::from_str(&env, env!("CARGO_PKG_VERSION"))
    }

    // ----- internal -----

    fn require_admin(env: &Env, admin: &Address) -> Result<(), RegistryError> {
        let stored = read_admin(env)?;
        common::require_admin(&stored, admin)?;
        Ok(())
    }
}
