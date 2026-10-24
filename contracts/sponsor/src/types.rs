use soroban_sdk::{contracterror, contracttype, Address, String, Vec};

/// Addresses and switches this contract works with.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// Address allowed to pause the desk and to hand the role on.
    pub admin: Address,
    /// The provider registry: service prices, service availability and the
    /// country a clinic is registered in.
    pub registry: Address,
    /// The voucher contract that escrows the money and runs the care
    /// lifecycle. A matching voucher is an ordinary voucher, funded by this
    /// contract.
    pub voucher: Address,
    /// The settlement token both contracts move.
    pub token: Address,
    /// While true, deposits and matches are rejected for everyone. The
    /// admin can still withdraw: pausing stops money going out, it does not
    /// trap money already here.
    pub paused: bool,
}

/// The conditions a sponsor sets for the vouchers it will match.
///
/// A sponsor picks the rules it wants and the desk honours them on every
/// match. The rules are the sponsor's own: one sponsor's cap never applies
/// to another's money.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rules {
    /// The match as a share of the matched voucher's amount, in basis
    /// points. `10_000` is one-for-one: the sponsor adds as much as the
    /// funder put in. `20_000` is two-for-one.
    pub match_bps: u32,
    /// The most the sponsor will add for any one voucher, whatever the
    /// ratio works out to.
    pub cap_per_voucher: i128,
    /// The least a voucher may be worth to qualify. Zero means no minimum.
    pub min_voucher_amount: i128,
    /// How long a voucher must have been funded before it qualifies, in
    /// seconds. Zero means a voucher can be matched as soon as it exists.
    ///
    /// This is the anti-farming wait: a voucher that appeared a second ago
    /// and is about to be claimed by the clinic that funded it cannot be
    /// matched in the same breath.
    pub min_voucher_age_secs: u64,
    /// The countries a matched clinic may be registered in, as ISO 3166-1
    /// alpha-2 codes. Empty means any country qualifies.
    pub eligible_countries: Vec<String>,
    /// The service codes that qualify. Empty means any service qualifies.
    pub eligible_service_codes: Vec<u32>,
}

/// A match the sponsor has made whose matching voucher is still open.
///
/// The desk keeps these so it can tell, later, whether the money went to
/// the clinic or came back.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenMatch {
    /// The id of the matching voucher this contract funded.
    pub matched_voucher_id: u64,
    /// What the match cost the sponsor.
    pub amount: i128,
}

/// One sponsor's budget and rules.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SponsorAccount {
    /// The sponsor this account belongs to.
    pub sponsor: Address,
    /// Everything the sponsor has ever deposited.
    pub deposited: i128,
    /// Everything the sponsor has ever committed to a match. A refunded
    /// match stays counted here; the refund is counted in `refunded`.
    pub committed: i128,
    /// Money that came back from a matching voucher and is available again.
    pub refunded: i128,
    /// Everything the sponsor has ever withdrawn.
    pub withdrawn: i128,
    /// The rules the sponsor set. A `match_bps` of zero means no rules have
    /// been set yet: a ratio of zero is not a rule the desk accepts, so it
    /// can stand for "not set" without a second field.
    pub rules: Rules,
    /// Matches whose matching voucher is still open.
    pub open_matches: Vec<OpenMatch>,
}

/// The desk's totals across every sponsor.
///
/// Kept next to the per-sponsor accounts so the solvency view does not have
/// to walk every sponsor.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Totals {
    pub deposited: i128,
    pub committed: i128,
    pub refunded: i128,
    pub withdrawn: i128,
}

/// A match, and the voucher it produced.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchRecord {
    /// The voucher that was matched.
    pub original_voucher_id: u64,
    /// The matching voucher this contract funded.
    pub matched_voucher_id: u64,
    /// The sponsor whose budget paid for it.
    pub sponsor: Address,
    /// What the match cost the sponsor.
    pub amount: i128,
    /// The clinic the care goes to.
    pub provider: Address,
    /// The service the care is for.
    pub service_code: u32,
    /// When the match was made.
    pub matched_at: u64,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Addresses and switches.
    Config,
    /// Address nominated by `propose_admin`, awaiting `accept_admin`.
    PendingAdmin,
    /// One sponsor's budget and rules.
    Sponsor(Address),
    /// The match a voucher received, keyed by the original voucher id.
    Match(u64),
    /// The original voucher id a matching voucher belongs to, keyed by the
    /// matching voucher id.
    OriginalOf(u64),
    /// Totals across every sponsor.
    Totals,
}

#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum SponsorError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    NotAuthorized = 3,
    /// The amount deposited or withdrawn was not positive.
    InvalidAmount = 4,
    /// The rules are outside what the desk will accept.
    InvalidRules = 5,
    /// The sponsor has not set any rules yet, so nothing is eligible.
    RulesNotSet = 6,
    /// The voucher does not exist in the voucher contract.
    VoucherNotFound = 7,
    /// The voucher is not `Funded`, so it cannot be matched.
    VoucherNotFunded = 8,
    /// The voucher has expired.
    VoucherExpired = 9,
    /// The voucher has already been matched.
    AlreadyMatched = 10,
    /// The sponsor tried to match a voucher it funded itself, or one this
    /// contract funded.
    SelfMatchNotAllowed = 11,
    /// The clinic is not registered in one of the sponsor's countries.
    CountryNotEligible = 12,
    /// The service is not one the sponsor matches.
    ServiceNotEligible = 13,
    /// The voucher was funded too recently to qualify.
    VoucherTooNew = 14,
    /// The voucher is worth less than the sponsor's minimum.
    VoucherTooSmall = 15,
    /// The match works out below the service price, and a voucher cannot be
    /// funded below the price of the care it pays for.
    MatchBelowPrice = 16,
    /// The sponsor's budget cannot cover this match.
    BudgetExhausted = 17,
    /// The withdrawal is larger than the sponsor's unused budget.
    WithdrawExceedsBudget = 18,
    /// The sponsor already has as many open matches as the desk allows.
    TooManyOpenMatches = 19,
    /// The desk is paused; deposits and matches are rejected.
    Paused = 20,
    /// The voucher contract refused the matching voucher.
    VoucherCallFailed = 21,
    /// The clinic is not active in the registry, so it cannot be paid.
    ProviderNotEligible = 22,
    /// The clinic has paused this service, so it cannot be funded.
    ServiceUnavailable = 23,
    /// The match works out to nothing, because the ratio is small and the
    /// voucher is small.
    MatchTooSmall = 24,
    /// A counter would have to exceed the largest amount the token can
    /// represent.
    MathOverflow = 25,
    /// There is no admin nomination waiting to be accepted.
    NoPendingAdmin = 26,
}

/// Maps the shared helpers' failures onto this contract's error numbers, so
/// callers keep seeing the same codes they did before the helpers were shared.
impl From<sci_common::CommonError> for SponsorError {
    fn from(error: sci_common::CommonError) -> Self {
        match error {
            sci_common::CommonError::NotAuthorized => SponsorError::NotAuthorized,
            sci_common::CommonError::NoPendingAdmin => SponsorError::NoPendingAdmin,
        }
    }
}

#[cfg(test)]
mod error_conversion_tests {
    use super::*;

    #[test]
    fn not_authorized_maps() {
        let err: SponsorError = sci_common::CommonError::NotAuthorized.into();
        assert_eq!(err, SponsorError::NotAuthorized);
    }

    #[test]
    fn no_pending_admin_maps() {
        let err: SponsorError = sci_common::CommonError::NoPendingAdmin.into();
        assert_eq!(err, SponsorError::NoPendingAdmin);
    }
}
