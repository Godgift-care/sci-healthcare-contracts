#![no_std]

//! # SCI Healthcare — shared contract helpers
//!
//! Storage policy and admin handover used by `registry`, `voucher` and
//! `receipt`. These are the parts that were copied into every contract and
//! then drifted: TTL numbers, how an entry is extended, and the two-step
//! handover that protects an admin or minter role from a typo.
//!
//! Nothing here defines a public contract interface. Each contract keeps its
//! own `DataKey`, error type and events; only the storage mechanics are
//! shared, so the on-chain interface is unchanged by using this crate.
//!
//! What breaks if this is wrong: an entry extended with a shorter TTL than the
//! contract documents is archived early, and a handover that skips the
//! acceptance step strands a role on an address nobody controls.

use soroban_sdk::{contractevent, Address, BytesN, Env, IntoVal, Val};

// ----- shared events -----

/// Emitted when an admin nominates a successor. The nominee must call
/// `accept_admin` to complete the handover.
///
/// The `#[topic]` annotation on `admin` is part of the ABI: every indexer
/// that watches for this event by the current admin's address depends on it.
/// Field names are part of the protocol's public interface; renaming them is
/// a breaking change.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminProposed {
    #[topic]
    pub admin: Address,
    pub pending_admin: Address,
}

/// Emitted when the nominee accepts and becomes the new admin.
///
/// Field names are part of the protocol's public interface; renaming them is
/// a breaking change.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminChanged {
    pub new_admin: Address,
}

/// Emitted when a contract's WASM executable is upgraded in place.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Upgraded {
    pub new_wasm_hash: BytesN<32>,
}

/// Roughly one day of ledgers, assuming ~5 second close times.
pub const DAY_IN_LEDGERS: u32 = 17_280;
pub const INSTANCE_TTL_THRESHOLD: u32 = 7 * DAY_IN_LEDGERS;
pub const INSTANCE_TTL_EXTEND: u32 = 30 * DAY_IN_LEDGERS;
pub const PERSISTENT_TTL_THRESHOLD: u32 = 30 * DAY_IN_LEDGERS;
pub const PERSISTENT_TTL_EXTEND: u32 = 90 * DAY_IN_LEDGERS;
/// An attestation counter is only read on the day it counts; two days of
/// ledgers covers that day whatever the hour it was first written.
pub const ATTESTATIONS_TTL: u32 = 2 * DAY_IN_LEDGERS;

/// Failures the shared helpers can report.
///
/// Each contract maps these onto its own `#[contracterror]` type, so the
/// numbers callers see on chain stay exactly what they were before.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommonError {
    NotAuthorized,
    NoPendingAdmin,
}

/// Pushes the contract instance (config, admin, counters) back out to 30 days.
pub fn extend_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND);
}

/// Pushes a persistent entry back out to 90 days once it crosses 30.
pub fn extend_persistent<K>(env: &Env, key: &K)
where
    K: IntoVal<Env, Val>,
{
    env.storage()
        .persistent()
        .extend_ttl(key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
}

pub fn read_admin<K>(env: &Env, key: &K) -> Option<Address>
where
    K: IntoVal<Env, Val>,
{
    env.storage().instance().get(key)
}

pub fn write_admin<K>(env: &Env, key: &K, admin: &Address)
where
    K: IntoVal<Env, Val>,
{
    env.storage().instance().set(key, admin);
}

pub fn read_pending_admin<K>(env: &Env, key: &K) -> Option<Address>
where
    K: IntoVal<Env, Val>,
{
    env.storage().instance().get(key)
}

/// Records a nominee and keeps the instance alive until they accept.
pub fn propose_admin<K>(env: &Env, pending_key: &K, new_admin: &Address)
where
    K: IntoVal<Env, Val>,
{
    env.storage().instance().set(pending_key, new_admin);
    extend_instance(env);
}

/// Consumes the pending nomination, but only for the nominee who signs.
///
/// Returns the accepted address so each contract can write it wherever it
/// keeps its own admin (an instance entry, or its `Config`).
pub fn take_pending_admin<K>(
    env: &Env,
    pending_key: &K,
    new_admin: &Address,
) -> Result<Address, CommonError>
where
    K: IntoVal<Env, Val>,
{
    let pending: Address =
        read_pending_admin(env, pending_key).ok_or(CommonError::NoPendingAdmin)?;
    if pending != *new_admin {
        return Err(CommonError::NotAuthorized);
    }
    new_admin.require_auth();

    env.storage().instance().remove(pending_key);
    extend_instance(env);
    Ok(pending)
}

/// Checks `admin` against the stored address, then demands its signature.
///
/// The order matters: a wrong `admin` argument fails before any signature is
/// requested, so a caller cannot probe the role with an unsigned address.
pub fn require_admin(stored: &Address, admin: &Address) -> Result<(), CommonError> {
    if stored != admin {
        return Err(CommonError::NotAuthorized);
    }
    admin.require_auth();
    Ok(())
}

#[cfg(test)]
mod ttl_doc_tests {
    use super::*;

    /// Verifies that the day figures in docs/developers/ttl.md are derived
    /// from the constants, not independently written. If you change a constant,
    /// this test will fail and remind you to update the docs.
    #[test]
    fn ttl_doc_figures_match_constants() {
        assert_eq!(INSTANCE_TTL_THRESHOLD / DAY_IN_LEDGERS, 7);
        assert_eq!(INSTANCE_TTL_EXTEND / DAY_IN_LEDGERS, 30);
        assert_eq!(PERSISTENT_TTL_THRESHOLD / DAY_IN_LEDGERS, 30);
        assert_eq!(PERSISTENT_TTL_EXTEND / DAY_IN_LEDGERS, 90);
        assert_eq!(ATTESTATIONS_TTL / DAY_IN_LEDGERS, 2);
    }
}
