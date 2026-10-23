use soroban_sdk::{contracterror, contracttype, Address, String};

/// Lifecycle state of a care provider in the registry.
///
/// A provider is `Pending` on self-registration and only becomes `Active`
/// after an off-chain verification step performed by an attester or the
/// admin. Vouchers can only be created against `Active` providers.
///
/// The lifecycle is a one-way street: `set_provider_status` only accepts
/// the transitions `Pending → Active`, `Pending → Suspended`,
/// `Active → Suspended` and `Suspended → Active` (anything else fails with
/// `InvalidTransition`). A provider that leaves for good calls `retire`,
/// which sets `Retired` — a terminal state nothing can move it out of.
/// New statuses are added, never renumbered.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderStatus {
    Pending = 0,
    Active = 1,
    Suspended = 2,
    /// Withdrawn from the registry by its own owner via `retire`. Not
    /// active, cannot receive vouchers, and no status change — by the admin
    /// or anyone else — can leave it: re-registering is the way back.
    Retired = 3,
}

impl ProviderStatus {
    /// Whether the registry may move a provider from `from` to `to` with
    /// `set_provider_status`.
    ///
    /// Only the four transitions that carry real information are allowed:
    /// `Pending → Active` (verified), `Pending → Suspended` (refused before
    /// it ever traded), `Active → Suspended` and `Suspended → Active`.
    /// Everything else — including setting the same status twice — is a
    /// no-op or backwards move that would emit a misleading event, and is
    /// refused with `InvalidTransition`. Entering `Retired` is not one of
    /// these: only the provider's own owner retires it, via `retire`.
    pub fn is_transition_allowed(from: ProviderStatus, to: ProviderStatus) -> bool {
        use ProviderStatus::{Active, Pending, Suspended};

        matches!(
            (from, to),
            (Pending, Active) | (Pending, Suspended) | (Active, Suspended) | (Suspended, Active)
        )
    }
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
    /// ISO 3166-1 alpha-2 country code, e.g. "NG", "KE". Two ASCII
    /// uppercase letters only.
    pub country: String,
    pub status: ProviderStatus,
    pub registered_at: u64,
    /// Reason code of the most recent status change, as documented in
    /// `docs/protocol/trust.md`. Codes are added, never renumbered. 0 on
    /// registration, before any status change has recorded a reason.
    pub reason_code: u32,
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
    /// Total number of registered providers.
    ProviderCount,
    /// Provider address at a registration index, in registration order.
    ProviderAt(u32),
    /// Address nominated by `propose_admin`, awaiting `accept_admin`.
    PendingAdmin,
    /// Most vouchers a clinic accepts outstanding at once; absent means
    /// unlimited.
    Capacity(Address),
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
    /// The requested provider status change is not one of the allowed
    /// transitions, or would leave `Retired`.
    InvalidTransition = 14,
}

/// Maps the shared helpers' failures onto this contract's error numbers, so
/// callers keep seeing the same codes they did before the helpers were shared.
impl From<sci_common::CommonError> for RegistryError {
    fn from(error: sci_common::CommonError) -> Self {
        match error {
            sci_common::CommonError::NotAuthorized => RegistryError::NotAuthorized,
            sci_common::CommonError::NoPendingAdmin => RegistryError::NoPendingAdmin,
        }
    }
}
