//! Typed contract events.
//!
//! Field names here are part of the protocol's public interface: the
//! indexer decodes these by name. Renaming a field is a breaking change.

use soroban_sdk::{contractevent, Address, String};

use crate::types::ProviderStatus;

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Initialized {
    pub admin: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderRegistered {
    #[topic]
    pub provider: Address,
    pub name: String,
    pub country: String,
    pub registered_at: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderStatusChanged {
    #[topic]
    pub provider: Address,
    pub status: ProviderStatus,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceUpserted {
    #[topic]
    pub provider: Address,
    #[topic]
    pub code: u32,
    pub label: String,
    pub price: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceRemoved {
    #[topic]
    pub provider: Address,
    pub code: u32,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttesterAdded {
    #[topic]
    pub attester: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttesterRemoved {
    #[topic]
    pub attester: Address,
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
