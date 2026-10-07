use soroban_sdk::{contracterror, contracttype, Address, String};

/// Lifecycle state of a care provider in the registry.
///
/// A provider is `Pending` on self-registration and only becomes `Active`
/// after an off-chain verification step performed by an attester or the
/// admin. Vouchers can only be created against `Active` providers.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderStatus {
    Pending = 0,
    Active = 1,
    Suspended = 2,
}

/// A registered care provider (clinic, pharmacy, diagnostic lab).
///
/// Deliberately contains no patient-identifying data. `owner` is the
/// address that receives settlement for this provider's vouchers.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Provider {
    pub owner: Address,
    pub name: String,
    /// ISO 3166-1 alpha-2 country code, e.g. "NG", "KE".
    pub country: String,
    pub status: ProviderStatus,
    pub registered_at: u64,
}

/// A single billable service offered by a provider.
///
/// `code` is a coarse service *category* (e.g. outpatient consult, malaria
/// RDT), never a diagnosis. Keeping this coarse is what keeps protected
/// health information off the ledger.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ServiceItem {
    pub code: u32,
    pub label: String,
    /// Price in the smallest unit of the settlement token.
    pub price: i128,
    pub active: bool,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Provider(Address),
    Service(Address, u32),
    Attester(Address),
    /// Address nominated by `propose_admin`, awaiting `accept_admin`.
    PendingAdmin,
}

#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum RegistryError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    NotAuthorized = 3,
    ProviderNotFound = 4,
    ProviderExists = 5,
    ProviderNotActive = 6,
    ServiceNotFound = 7,
    InvalidPrice = 8,
    InvalidCountry = 9,
    EmptyName = 10,
    NoPendingAdmin = 11,
}
