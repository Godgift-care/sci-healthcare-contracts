#![no_std]
// A panic aborts the whole invocation with an opaque host error. Contract
// code must return a typed error instead, so both are denied outside tests.
// Tests keep them for brevity: they are fail-fast assertions, not contract
// behaviour.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

//! # SCI Healthcare — Test USDC Faucet (testnet only)
//!
//! Lets anyone trying the testnet demo get a small amount of the demo
//! settlement token, without a server holding the issuer key.
//!
//! The token's Stellar Asset Contract names this contract as its admin, so
//! `drip` can mint. Each address may drip once per cooldown, the amount is
//! fixed by the faucet admin, and an optional lifetime cap stops one address
//! draining the faucet slowly forever. The admin can hand token administration
//! back at any time with `release_token_admin`.
//!
//! **Never deploy this against a real asset.** Admin rights over a real
//! token here would let anyone mint it.

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, token, Address, Env, String,
};

#[cfg(test)]
mod test;

const DAY_IN_LEDGERS: u32 = 17_280;
const INSTANCE_TTL_THRESHOLD: u32 = 7 * DAY_IN_LEDGERS;
const INSTANCE_TTL_EXTEND: u32 = 30 * DAY_IN_LEDGERS;
/// Drip records only need to outlive the longest sensible cooldown.
const DRIP_TTL_THRESHOLD: u32 = DAY_IN_LEDGERS;
const DRIP_TTL_EXTEND: u32 = 7 * DAY_IN_LEDGERS;
/// Lifetime drip totals are kept for as long as the faucet holds token admin.
const LIFETIME_TTL_THRESHOLD: u32 = 30 * DAY_IN_LEDGERS;
const LIFETIME_TTL_EXTEND: u32 = 90 * DAY_IN_LEDGERS;
/// Seconds in a UTC day: buckets the shared daily-cap counter.
const SECONDS_PER_DAY: u64 = 24 * 60 * 60;

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct FaucetConfig {
    pub admin: Address,
    /// Stellar Asset Contract of the demo token. This faucet must be its admin.
    pub token: Address,
    /// Base units minted per drip.
    pub amount: i128,
    /// Seconds an address must wait between drips.
    pub cooldown: u64,
    /// Base units the faucet may mint per UTC day, across all addresses
    /// combined. Resets when the day changes.
    pub daily_cap: i128,
    /// Base units one address may ever receive from the faucet. Zero means
    /// unlimited; any other value is a hard lifetime cap.
    pub lifetime_cap: i128,
    /// While true, `drip` is rejected for everyone. Toggled with
    /// `set_paused`; pausing does not give up the token admin role.
    pub paused: bool,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Config,
    /// Ledger time of an address's last drip.
    LastDrip(Address),
    /// (`day`, `minted`) running total for the current UTC day, shared by
    /// every address. Reset when the day changes.
    DailyCap,
    /// Total base units an address has ever received; never reset.
    LifetimeDripped(Address),
}

#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum FaucetError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    NotAuthorized = 3,
    InvalidAmount = 4,
    CoolingDown = 5,
    /// The daily minting cap across all addresses has been reached.
    DailyCapReached = 6,
    /// The faucet is paused; `drip` is rejected until the admin unpauses.
    Paused = 7,
    /// This address has received its lifetime allowance.
    LifetimeCapReached = 8,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dripped {
    #[topic]
    pub to: Address,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DripChanged {
    pub amount: i128,
    pub cooldown: u64,
}

#[contract]
pub struct Faucet;

#[contractimpl]
impl Faucet {
    /// Sets the admin, token, drip amount, cooldown, daily cap and lifetime
    /// cap per address. Callable once.
    ///
    /// `daily_cap` is the maximum number of base units the faucet may mint per
    /// UTC day across all addresses combined. The per-address cooldown still
    /// applies on top of it. `lifetime_cap` is the maximum number of base units
    /// one address may ever receive; `0` means unlimited.
    pub fn initialize(
        env: Env,
        admin: Address,
        token: Address,
        amount: i128,
        cooldown: u64,
        daily_cap: i128,
        lifetime_cap: i128,
    ) -> Result<(), FaucetError> {
        if env.storage().instance().has(&DataKey::Config) {
            return Err(FaucetError::AlreadyInitialized);
        }
        if amount <= 0 || daily_cap <= 0 || lifetime_cap < 0 {
            return Err(FaucetError::InvalidAmount);
        }
        Self::write_config(
            &env,
            &FaucetConfig {
                admin,
                token,
                amount,
                cooldown,
                daily_cap,
                lifetime_cap,
                paused: false,
            },
        );
        Ok(())
    }

    /// Mints the drip amount to `to`, at most once per cooldown, while the
    /// shared daily cap has not been reached and `to` is under its lifetime
    /// cap (if one is configured).
    ///
    /// `to` must sign, so one wallet cannot drain the faucet on behalf of
    /// many addresses it does not control. A classic account also needs a
    /// trustline for the token, as for any Stellar asset.
    pub fn drip(env: Env, to: Address) -> Result<i128, FaucetError> {
        let config = Self::read_config(&env)?;
        if config.paused {
            return Err(FaucetError::Paused);
        }
        to.require_auth();

        let now = env.ledger().timestamp();

        let key = DataKey::LastDrip(to.clone());
        if let Some(last) = env.storage().temporary().get::<_, u64>(&key) {
            if now < last.saturating_add(config.cooldown) {
                return Err(FaucetError::CoolingDown);
            }
        }

        // A lifetime cap is per address and never resets. Zero means the
        // admin did not set one.
        let lifetime_dripped = Self::read_lifetime_dripped(&env, &to);
        if config.lifetime_cap > 0
            && lifetime_dripped.saturating_add(config.amount) > config.lifetime_cap
        {
            return Err(FaucetError::LifetimeCapReached);
        }

        // Daily cap is shared by every address and resets at day boundaries.
        let day = now / SECONDS_PER_DAY;
        let cap_key = DataKey::DailyCap;
        let mut state: (u64, i128) = env.storage().instance().get(&cap_key).unwrap_or((0, 0));
        if state.0 != day {
            state = (day, 0);
        }
        if state.1.saturating_add(config.amount) > config.daily_cap {
            return Err(FaucetError::DailyCapReached);
        }

        token::StellarAssetClient::new(&env, &config.token).mint(&to, &config.amount);

        env.storage().temporary().set(&key, &now);
        env.storage()
            .temporary()
            .extend_ttl(&key, DRIP_TTL_THRESHOLD, DRIP_TTL_EXTEND);
        Self::write_lifetime_dripped(&env, &to, lifetime_dripped.saturating_add(config.amount));
        state.1 = state.1.saturating_add(config.amount);
        env.storage().instance().set(&cap_key, &state);
        Self::extend_instance(&env);

        Dripped {
            to,
            amount: config.amount,
        }
        .publish(&env);
        Ok(config.amount)
    }

    /// Changes the drip amount and cooldown. Admin only.
    pub fn set_drip(
        env: Env,
        admin: Address,
        amount: i128,
        cooldown: u64,
    ) -> Result<(), FaucetError> {
        let mut config = Self::require_admin(&env, &admin)?;
        if amount <= 0 {
            return Err(FaucetError::InvalidAmount);
        }
        config.amount = amount;
        config.cooldown = cooldown;
        Self::write_config(&env, &config);
        DripChanged { amount, cooldown }.publish(&env);
        Ok(())
    }

    /// Pauses or unpauses the faucet. While paused, `drip` is rejected for
    /// everyone. Pausing does not give up the token admin role. Admin only.
    pub fn set_paused(env: Env, admin: Address, paused: bool) -> Result<(), FaucetError> {
        let mut config = Self::require_admin(&env, &admin)?;
        config.paused = paused;
        Self::write_config(&env, &config);
        Ok(())
    }

    /// Hands administration of the token to `new_admin`, normally back to
    /// the issuer. The faucet stops working afterwards. Admin only.
    pub fn release_token_admin(
        env: Env,
        admin: Address,
        new_admin: Address,
    ) -> Result<(), FaucetError> {
        let config = Self::require_admin(&env, &admin)?;
        token::StellarAssetClient::new(&env, &config.token).set_admin(&new_admin);
        Ok(())
    }

    // ----- views -----

    pub fn get_config(env: Env) -> Result<FaucetConfig, FaucetError> {
        Self::read_config(&env)
    }

    /// Earliest ledger time `to` may drip again; 0 if it may drip now.
    pub fn next_drip_at(env: Env, to: Address) -> Result<u64, FaucetError> {
        let config = Self::read_config(&env)?;
        let last: Option<u64> = env.storage().temporary().get(&DataKey::LastDrip(to));
        Ok(match last {
            Some(t) if env.ledger().timestamp() < t.saturating_add(config.cooldown) => {
                t.saturating_add(config.cooldown)
            }
            _ => 0,
        })
    }

    /// Crate version this contract was built from, e.g. `"0.2.0"`.
    ///
    /// Lets integrators and the indexer tell deployments apart without
    /// comparing wasm hashes.
    pub fn version(env: Env) -> String {
        String::from_str(&env, env!("CARGO_PKG_VERSION"))
    }

    // ----- internal -----

    /// Base units `to` has ever received. Extends the persistent entry when
    /// it exists, so a lifetime total does not lapse while the faucet is live.
    fn read_lifetime_dripped(env: &Env, to: &Address) -> i128 {
        let key = DataKey::LifetimeDripped(to.clone());
        let storage = env.storage().persistent();
        if !storage.has(&key) {
            return 0;
        }
        storage.extend_ttl(&key, LIFETIME_TTL_THRESHOLD, LIFETIME_TTL_EXTEND);
        storage.get(&key).unwrap_or(0)
    }

    fn write_lifetime_dripped(env: &Env, to: &Address, total: i128) {
        let key = DataKey::LifetimeDripped(to.clone());
        let storage = env.storage().persistent();
        storage.set(&key, &total);
        storage.extend_ttl(&key, LIFETIME_TTL_THRESHOLD, LIFETIME_TTL_EXTEND);
    }

    fn read_config(env: &Env) -> Result<FaucetConfig, FaucetError> {
        env.storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(FaucetError::NotInitialized)
    }

    fn write_config(env: &Env, config: &FaucetConfig) {
        env.storage().instance().set(&DataKey::Config, config);
        Self::extend_instance(env);
    }

    fn extend_instance(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND);
    }

    fn require_admin(env: &Env, admin: &Address) -> Result<FaucetConfig, FaucetError> {
        let config = Self::read_config(env)?;
        if config.admin != *admin {
            return Err(FaucetError::NotAuthorized);
        }
        admin.require_auth();
        Ok(config)
    }
}
