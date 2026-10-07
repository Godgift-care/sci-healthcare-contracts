#![no_std]

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

use soroban_sdk::{contract, contractimpl, Address, Env, String, Vec};

pub use events::{
    AdminChanged, AdminProposed, AttesterAdded, AttesterRemoved, Initialized, ProviderRegistered,
    ProviderStatusChanged, ServiceRemoved, ServiceUpserted,
};
pub use types::{DataKey, Provider, ProviderStatus, RegistryError, ServiceItem};

use storage::{
    clear_pending_admin, extend_instance, has_admin, has_provider, read_admin, read_attester,
    read_pending_admin, read_provider, read_service, remove_service as storage_remove_service,
    write_admin, write_attester, write_pending_admin, write_provider, write_service,
};

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
    pub fn set_provider_status(
        env: Env,
        admin: Address,
        provider_addr: Address,
        status: ProviderStatus,
    ) -> Result<(), RegistryError> {
        Self::require_admin(&env, &admin)?;

        let mut provider = read_provider(&env, &provider_addr)?;
        provider.status = status;
        write_provider(&env, &provider_addr, &provider);
        extend_instance(&env);

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

        let item = ServiceItem {
            code,
            label: label.clone(),
            price,
            active: true,
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
        write_pending_admin(&env, &new_admin);
        extend_instance(&env);
        AdminProposed {
            admin,
            pending_admin: new_admin,
        }
        .publish(&env);
        Ok(())
    }

    /// The nominated address accepts and becomes admin.
    pub fn accept_admin(env: Env, new_admin: Address) -> Result<(), RegistryError> {
        let pending = read_pending_admin(&env).ok_or(RegistryError::NoPendingAdmin)?;
        if pending != new_admin {
            return Err(RegistryError::NotAuthorized);
        }
        new_admin.require_auth();

        write_admin(&env, &new_admin);
        clear_pending_admin(&env);
        extend_instance(&env);
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

    /// The check `voucher` relies on before escrowing any funds.
    pub fn is_active_provider(env: Env, provider_addr: Address) -> bool {
        match read_provider(&env, &provider_addr) {
            Ok(p) => p.status == ProviderStatus::Active,
            Err(_) => false,
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
    /// Returns a primitive rather than a struct so the voucher contract can
    /// validate pricing over a cross-contract interface without having to
    /// redeclare this contract's types.
    pub fn get_service_price(env: Env, provider_addr: Address, code: u32) -> i128 {
        match read_service(&env, &provider_addr, code) {
            Ok(item) if item.active => item.price,
            _ => 0,
        }
    }

    pub fn is_attester(env: Env, addr: Address) -> bool {
        read_attester(&env, &addr)
    }

    // ----- internal -----

    fn require_admin(env: &Env, admin: &Address) -> Result<(), RegistryError> {
        let stored = read_admin(env)?;
        if stored != *admin {
            return Err(RegistryError::NotAuthorized);
        }
        admin.require_auth();
        Ok(())
    }
}
