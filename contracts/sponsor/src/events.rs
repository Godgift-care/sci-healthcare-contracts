//! Typed contract events.
//!
//! Field names here are part of the protocol's public interface: the
//! indexer decodes these by name. Renaming a field is a breaking change.

use soroban_sdk::{contractevent, Address};

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Initialized {
    pub admin: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BudgetDeposited {
    #[topic]
    pub sponsor: Address,
    /// What was deposited.
    pub amount: i128,
    /// The sponsor's unused budget after the deposit.
    pub available: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RulesSet {
    #[topic]
    pub sponsor: Address,
    /// The match ratio, in basis points.
    pub match_bps: u32,
    /// The most the sponsor will add for one voucher.
    pub cap_per_voucher: i128,
    /// The least a voucher may be worth to qualify.
    pub min_voucher_amount: i128,
    /// How long a voucher must have been funded before it qualifies.
    pub min_voucher_age_secs: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MatchCreated {
    #[topic]
    pub sponsor: Address,
    #[topic]
    pub original_voucher_id: u64,
    /// The matching voucher this contract funded.
    pub matched_voucher_id: u64,
    /// The clinic the care goes to.
    pub provider: Address,
    /// The service the care is for.
    pub service_code: u32,
    /// What the match cost the sponsor.
    pub amount: i128,
    /// When the match was made.
    pub matched_at: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MatchSettled {
    #[topic]
    pub sponsor: Address,
    /// The matching voucher that was settled.
    pub matched_voucher_id: u64,
    /// What the sponsor's budget was spent on.
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MatchRefunded {
    #[topic]
    pub sponsor: Address,
    /// The matching voucher that was refunded.
    pub matched_voucher_id: u64,
    /// What came back to the sponsor's budget.
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BudgetWithdrawn {
    #[topic]
    pub sponsor: Address,
    /// What was withdrawn.
    pub amount: i128,
    /// The sponsor's unused budget after the withdrawal.
    pub available: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminProposed {
    #[topic]
    pub admin: Address,
    pub pending_admin: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminChanged {
    pub new_admin: Address,
}
