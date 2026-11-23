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

use soroban_sdk::{contractevent, Address, Env, IntoVal, Val};

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

// ----- pagination -----

/// Returns the `(start, end)` range to iterate for a paginated view, or
/// `None` when the page would be empty.
///
/// `limit` is clamped to `max` first. `start + clamped_limit` is computed
/// with saturating arithmetic to avoid overflow, then clamped to `total`, so
/// callers never read past the end of the index.
///
/// Every paginated view in the protocol calls this once and iterates
/// `start..end`, which keeps the clamping and overflow logic in one place.
/// `max` is the per-view constant (e.g. `MAX_PROVIDERS_PAGE`).
pub fn page_bounds(start: u32, limit: u32, total: u32, max: u32) -> Option<(u32, u32)> {
    if start >= total {
        return None;
    }
    let clamped = limit.min(max);
    if clamped == 0 {
        return None;
    }
    let end = start.saturating_add(clamped).min(total);
    Some((start, end))
}

// ----- tests -----

#[cfg(test)]
mod test {
    use soroban_sdk::{
        contract, contractimpl, contracttype, testutils::Address as _, Address, Env,
    };

    use crate::{
        extend_instance, extend_persistent, page_bounds, propose_admin, read_pending_admin,
        require_admin, take_pending_admin, CommonError,
    };

    /// Minimal key type used only in tests — real contracts supply their own.
    #[contracttype]
    enum Key {
        Admin,
        Pending,
        Data,
    }

    /// A no-op contract used only to get a valid contract context for storage
    /// operations, which require `env.as_contract()`.
    #[contract]
    struct Ctx;

    #[contractimpl]
    impl Ctx {}

    fn ctx(env: &Env) -> Address {
        env.register(Ctx, ())
    }

    // ----- propose_admin -----

    #[test]
    fn propose_stores_the_nominee() {
        let env = Env::default();
        env.mock_all_auths();
        let id = ctx(&env);
        let new_admin = Address::generate(&env);

        env.as_contract(&id, || {
            propose_admin(&env, &Key::Pending, &new_admin);
            assert_eq!(read_pending_admin(&env, &Key::Pending), Some(new_admin));
        });
    }

    #[test]
    fn proposing_twice_replaces_the_nominee() {
        let env = Env::default();
        env.mock_all_auths();
        let id = ctx(&env);
        let first = Address::generate(&env);
        let second = Address::generate(&env);

        env.as_contract(&id, || {
            propose_admin(&env, &Key::Pending, &first);
            propose_admin(&env, &Key::Pending, &second);
            assert_eq!(read_pending_admin(&env, &Key::Pending), Some(second));
        });
    }

    // ----- take_pending_admin -----

    #[test]
    fn take_pending_admin_accepts_the_correct_nominee() {
        let env = Env::default();
        env.mock_all_auths();
        let id = ctx(&env);
        let new_admin = Address::generate(&env);

        env.as_contract(&id, || {
            propose_admin(&env, &Key::Pending, &new_admin);
            let accepted = take_pending_admin(&env, &Key::Pending, &new_admin).unwrap();
            assert_eq!(accepted, new_admin);
            // Nomination is consumed.
            assert_eq!(read_pending_admin(&env, &Key::Pending), None);
        });
    }

    #[test]
    fn take_pending_admin_rejects_wrong_nominee() {
        let env = Env::default();
        env.mock_all_auths();
        let id = ctx(&env);
        let real_nominee = Address::generate(&env);
        let impostor = Address::generate(&env);

        env.as_contract(&id, || {
            propose_admin(&env, &Key::Pending, &real_nominee);
            let err = take_pending_admin(&env, &Key::Pending, &impostor).unwrap_err();
            assert_eq!(err, CommonError::NotAuthorized);
            // Nomination is not consumed.
            assert_eq!(read_pending_admin(&env, &Key::Pending), Some(real_nominee));
        });
    }

    #[test]
    fn take_pending_admin_errors_when_nothing_proposed() {
        let env = Env::default();
        env.mock_all_auths();
        let id = ctx(&env);
        let addr = Address::generate(&env);

        env.as_contract(&id, || {
            let err = take_pending_admin(&env, &Key::Pending, &addr).unwrap_err();
            assert_eq!(err, CommonError::NoPendingAdmin);
        });
    }

    // ----- require_admin -----

    #[test]
    fn require_admin_passes_for_the_stored_address() {
        let env = Env::default();
        env.mock_all_auths();
        let id = ctx(&env);
        let admin = Address::generate(&env);

        // require_auth needs a contract context too.
        env.as_contract(&id, || {
            assert!(require_admin(&admin, &admin).is_ok());
        });
    }

    #[test]
    fn require_admin_fails_for_a_different_address() {
        let env = Env::default();
        env.mock_all_auths();
        let stored = Address::generate(&env);
        let caller = Address::generate(&env);

        // No contract context needed: the error fires before require_auth.
        let err = require_admin(&stored, &caller).unwrap_err();
        assert_eq!(err, CommonError::NotAuthorized);
    }

    // ----- TTL helpers -----

    #[test]
    fn extend_instance_does_not_panic() {
        let env = Env::default();
        env.mock_all_auths();
        let id = ctx(&env);
        env.as_contract(&id, || {
            extend_instance(&env);
        });
    }

    #[test]
    fn extend_persistent_does_not_panic_for_present_key() {
        let env = Env::default();
        env.mock_all_auths();
        let id = ctx(&env);
        let addr = Address::generate(&env);
        env.as_contract(&id, || {
            env.storage().persistent().set(&Key::Data, &addr);
            extend_persistent(&env, &Key::Data);
        });
    }

    // ----- page_bounds -----

    #[test]
    fn page_bounds_returns_full_range_when_within_total() {
        assert_eq!(page_bounds(0, 10, 50, 50), Some((0, 10)));
    }

    #[test]
    fn page_bounds_clamps_limit_to_max() {
        assert_eq!(page_bounds(0, 100, 50, 25), Some((0, 25)));
    }

    #[test]
    fn page_bounds_clamps_end_to_total() {
        assert_eq!(page_bounds(45, 10, 50, 50), Some((45, 50)));
    }

    #[test]
    fn page_bounds_returns_none_when_start_equals_total() {
        assert_eq!(page_bounds(50, 10, 50, 50), None);
    }

    #[test]
    fn page_bounds_returns_none_when_start_exceeds_total() {
        assert_eq!(page_bounds(60, 10, 50, 50), None);
    }

    #[test]
    fn page_bounds_returns_none_when_limit_is_zero() {
        assert_eq!(page_bounds(0, 0, 50, 50), None);
    }

    #[test]
    fn page_bounds_returns_none_when_max_is_zero() {
        assert_eq!(page_bounds(0, 10, 50, 0), None);
    }

    #[test]
    fn page_bounds_handles_start_plus_limit_overflow() {
        // u32::MAX start is already beyond any sane total, so returns None.
        assert_eq!(page_bounds(u32::MAX, u32::MAX, 100, u32::MAX), None);
        // u32::MAX limit saturates to total.
        assert_eq!(page_bounds(0, u32::MAX, 100, u32::MAX), Some((0, 100)));
    }

    #[test]
    fn page_bounds_total_zero_always_returns_none() {
        assert_eq!(page_bounds(0, 10, 0, 50), None);
    }
}
