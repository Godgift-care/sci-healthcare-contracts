use soroban_sdk::{contracterror, contracttype, Address, BytesN, String};

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
    pub category: ProviderCategory,
    /// Hash of the verification evidence bundle (e.g. licence, inspection report).
    pub verification_evidence: Option<BytesN<32>>,
}

/// Category of a care provider in the registry.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderCategory {
    Clinic = 0,
    Pharmacy = 1,
    Laboratory = 2,
    Maternity = 3,
    Other = 4,
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
    /// Total number of active attesters in the index.
    AttesterCount,
    /// Attester address at index in addition order (swap-and-pop on removal).
    AttesterAt(u32),
    /// Total number of registered providers.
    ProviderCount,
    /// Provider address at a registration index, in registration order.
    ProviderAt(u32),
    /// Address nominated by `propose_admin`, awaiting `accept_admin`.
    PendingAdmin,
    /// Most vouchers a clinic accepts outstanding at once; absent means
    /// unlimited.
    Capacity(Address),
    /// Number of active providers in a country, for pagination.
    CountryCount(String),
    /// Address of the active provider at `index` in a country's list, in
    /// insertion order. Uses swap-and-pop on removal so reads stay O(1).
    CountryAt(String, u32),
    /// Number of providers in a given status.
    StatusCount(ProviderStatus),
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
    /// A scheduled price of `MAX_PRICE_JUMP` times the current price or more
    /// needs the explicit `confirm_large_change` flag. Large jumps are almost
    /// always a typo, and a typo here overcharges every funder that day.
    LargePriceJumpNeedsConfirmation = 15,
    /// `set_provider_status` was called with `reason_code` 0. Code 0 means
    /// "no status change yet" and is only ever written by registration, so a
    /// real change — above all reinstating a suspended clinic — must say why.
    MissingReasonCode = 16,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_authorized_maps() {
        let err: RegistryError = sci_common::CommonError::NotAuthorized.into();
        assert_eq!(err, RegistryError::NotAuthorized);
    }

    #[test]
    fn no_pending_admin_maps() {
        let err: RegistryError = sci_common::CommonError::NoPendingAdmin.into();
        assert_eq!(err, RegistryError::NoPendingAdmin);
    }

    #[test]
    fn provider_status_transitions() {
        use ProviderStatus::*;
        assert!(ProviderStatus::is_transition_allowed(Pending, Active));
        assert!(ProviderStatus::is_transition_allowed(Pending, Suspended));
        assert!(ProviderStatus::is_transition_allowed(Active, Suspended));
        assert!(ProviderStatus::is_transition_allowed(Suspended, Active));
        assert!(!ProviderStatus::is_transition_allowed(Pending, Pending));
        assert!(!ProviderStatus::is_transition_allowed(Active, Active));
        assert!(!ProviderStatus::is_transition_allowed(Suspended, Suspended));
        assert!(!ProviderStatus::is_transition_allowed(Active, Pending));
        assert!(!ProviderStatus::is_transition_allowed(Suspended, Pending));
        assert!(!ProviderStatus::is_transition_allowed(Pending, Retired));
        assert!(!ProviderStatus::is_transition_allowed(Retired, Active));
        assert!(!ProviderStatus::is_transition_allowed(Retired, Suspended));
        assert!(!ProviderStatus::is_transition_allowed(Retired, Retired));
    }

    #[test]
    fn provider_category_variants() {
        let categories = [
            ProviderCategory::Clinic,
            ProviderCategory::Pharmacy,
            ProviderCategory::Laboratory,
            ProviderCategory::Maternity,
            ProviderCategory::Other,
        ];
        for (i, c) in categories.iter().enumerate() {
            assert_eq!(*c as u32, i as u32);
        }
    }

    #[test]
    fn service_item_due_price() {
        let env = soroban_sdk::Env::default();
        let mut item = ServiceItem {
            code: 1,
            label: String::from_str(&env, "Consult"),
            price: 100,
            active: true,
            available: true,
            pending_price: 200,
            effective_at: 1000,
        };
        // Before effective_at: no change
        item.apply_due_price(999);
        assert_eq!(item.price, 100);
        assert_eq!(item.pending_price, 200);

        // At effective_at: price updated
        item.apply_due_price(1000);
        assert_eq!(item.price, 200);
        assert_eq!(item.pending_price, 0);

        // When pending_price is 0: no change
        item.apply_due_price(2000);
        assert_eq!(item.price, 200);
    }
}
