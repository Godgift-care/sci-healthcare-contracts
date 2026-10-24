//! Typed contract events.
//!
//! Field names here are part of the protocol's public interface: the
//! indexer decodes these by name. Renaming a field is a breaking change.

use soroban_sdk::{contractevent, Address, BytesN};

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Initialized {
    pub admin: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoucherCreated {
    #[topic]
    pub funder: Address,
    #[topic]
    pub provider: Address,
    /// Indexed so a patient can find every voucher funded for them,
    /// including ones not yet settled.
    #[topic]
    pub beneficiary_ref: BytesN<32>,
    pub voucher_id: u64,
    pub service_code: u32,
    pub amount: i128,
    pub created_at: u64,
    pub expires_at: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoucherClaimed {
    #[topic]
    pub provider: Address,
    pub voucher_id: u64,
    pub claimed_at: u64,
    /// Appended, after the original fields: what was claimed, in the
    /// voucher's own event, so a single event needs no storage lookup.
    pub service_code: u32,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoucherAttested {
    #[topic]
    pub attester: Address,
    pub voucher_id: u64,
    pub attested_at: u64,
    pub dispute_deadline: u64,
    /// Appended, after the original fields; see `VoucherClaimed`.
    pub service_code: u32,
    pub amount: i128,
    /// Appended: attestations this attester has left today (UTC), so a
    /// burst from one key is visible to the indexer as it happens.
    pub remaining_today: u32,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoucherDisputed {
    #[topic]
    pub funder: Address,
    pub voucher_id: u64,
    pub reason_code: u32,
    /// Appended, after the original fields; see `VoucherClaimed`.
    pub service_code: u32,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoucherSettled {
    #[topic]
    pub provider: Address,
    pub voucher_id: u64,
    pub net: i128,
    pub fee: i128,
    /// Appended, after the original fields: the gross amount escrowed,
    /// beside the `net` and `fee` that were paid out.
    pub service_code: u32,
    pub amount: i128,
    /// Appended: the part of `fee` routed to the attester. The fee account
    /// received `fee - attester_share`, so `net + fee == amount`.
    pub attester_share: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoucherRefunded {
    #[topic]
    pub funder: Address,
    pub voucher_id: u64,
    /// Already carried by this event; left where it is so existing
    /// consumers keep reading the same field.
    pub amount: i128,
    /// Appended, after the original fields; see `VoucherClaimed`.
    pub service_code: u32,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoucherAmountReduced {
    #[topic]
    pub funder: Address,
    pub voucher_id: u64,
    /// Appended, after the original fields; see `VoucherClaimed`.
    pub service_code: u32,
    /// The amount escrowed before this call.
    pub previous_amount: i128,
    /// The amount escrowed now.
    pub amount: i128,
    /// What went back to the funder: `previous_amount - amount`.
    pub refunded: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoucherRedirected {
    #[topic]
    pub funder: Address,
    #[topic]
    pub new_provider: Address,
    pub voucher_id: u64,
    pub previous_provider: Address,
    pub service_code: u32,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeResolved {
    #[topic]
    pub admin: Address,
    pub voucher_id: u64,
    /// `true` only when the funder received the whole voucher back.
    pub refunded_funder: bool,
    /// Appended: the gross amount awarded to the clinic, before the fee.
    /// Zero on a full refund, the whole voucher on a full payout, and
    /// anything between for `resolve_split`.
    pub to_provider: i128,
    /// Appended: the protocol fee deducted from `to_provider`.
    pub fee: i128,
    /// Appended: the remainder returned to the funder.
    pub to_funder: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttestationCapChanged {
    #[topic]
    pub admin: Address,
    pub cap: u32,
}

/// The admin changed the range a funder's per-voucher dispute window must
/// fall in.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeWindowBoundsChanged {
    #[topic]
    pub admin: Address,
    pub min: u64,
    pub max: u64,
}

/// The admin changed the most value one clinic may hold in escrow. `max` is
/// 0 for unlimited.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowCapChanged {
    #[topic]
    pub admin: Address,
    pub max: i128,
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
