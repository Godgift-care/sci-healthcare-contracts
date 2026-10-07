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
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoucherAttested {
    #[topic]
    pub attester: Address,
    pub voucher_id: u64,
    pub attested_at: u64,
    pub dispute_deadline: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoucherDisputed {
    #[topic]
    pub funder: Address,
    pub voucher_id: u64,
    pub reason_code: u32,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoucherSettled {
    #[topic]
    pub provider: Address,
    pub voucher_id: u64,
    pub net: i128,
    pub fee: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoucherRefunded {
    #[topic]
    pub funder: Address,
    pub voucher_id: u64,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeResolved {
    #[topic]
    pub admin: Address,
    pub voucher_id: u64,
    pub refunded_funder: bool,
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
