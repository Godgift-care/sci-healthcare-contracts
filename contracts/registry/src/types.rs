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
    /// Whether the clinic can deliver this service right now. A clinic that
    /// runs out of a test kit pauses the service instead of delisting it, so
    /// it keeps its catalogue entry. Defaults to `true` on first listing.
    pub available: bool,
    /// Price scheduled by `schedule_service_price`, or 0 if none. Replaces
    /// `price` once the ledger reaches `effective_at`.
    pub pending_price: i128,
    /// When `pending_price` takes effect. 0 if nothing is scheduled.
    pub effective_at: u64,
}

impl ServiceItem {
    /// Applies a scheduled price whose time has come.
    ///
    /// Done on every read rather than by a keeper, so the new price takes
    /// effect at exactly `effective_at` without anyone having to call in.
    pub(crate) fn apply_due_price(&mut self, now: u64) {
        if self.pending_price > 0 && now >= self.effective_at {
            self.price = self.pending_price;
            self.pending_price = 0;
            self.effective_at = 0;
        }
    }
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
    /// A scheduled price must give funders at least `MIN_PRICE_NOTICE_SECS`.
    PriceNoticeTooShort = 12,
    /// `upsert_service` may lower a listed price at once, but a rise has to
    /// go through `schedule_service_price`.
    PriceIncreaseNeedsNotice = 13,
}
