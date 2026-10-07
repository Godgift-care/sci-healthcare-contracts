#![no_std]

//! # SCI Healthcare — Test USDC Faucet (testnet only)
//!
//! Lets anyone trying the testnet demo get a small amount of the demo
//! settlement token, without a server holding the issuer key.
//!
//! The token's Stellar Asset Contract names this contract as its admin, so
//! `drip` can mint. Each address may drip once per cooldown, and the amount
//! is fixed by the faucet admin. The admin can hand token administration
//! back at any time with `release_token_admin`.
//!
//! **Never deploy this against a real asset.** Admin rights over a real
//! token here would let anyone mint it.

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, token, Address, Env,
};

#[cfg(test)]
mod test;

const DAY_IN_LEDGERS: u32 = 17_280;
const INSTANCE_TTL_THRESHOLD: u32 = 7 * DAY_IN_LEDGERS;
const INSTANCE_TTL_EXTEND: u32 = 30 * DAY_IN_LEDGERS;
/// Drip records only need to outlive the longest sensible cooldown.
const DRIP_TTL_THRESHOLD: u32 = DAY_IN_LEDGERS;
const DRIP_TTL_EXTEND: u32 = 7 * DAY_IN_LEDGERS;

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
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Config,
    /// Ledger time of an address's last drip.
    LastDrip(Address),
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
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dripped {
    #[topic]
    pub to: Address,
    pub amount: i128,
}

#[contract]
pub struct Faucet;

#[contractimpl]
impl Faucet {
    /// Sets the admin, token, drip amount and cooldown. Callable once.
    pub fn initialize(
        env: Env,
        admin: Address,
        token: Address,
        amount: i128,
        cooldown: u64,
    ) -> Result<(), FaucetError> {
        if env.storage().instance().has(&DataKey::Config) {
            return Err(FaucetError::AlreadyInitialized);
        }
        if amount <= 0 {
            return Err(FaucetError::InvalidAmount);
        }
        Self::write_config(
            &env,
            &FaucetConfig {
                admin,
                token,
                amount,
                cooldown,
            },
        );
        Ok(())
    }

    /// Mints the drip amount to `to`, at most once per cooldown.
    ///
    /// `to` must sign, so one wallet cannot drain the faucet on behalf of
    /// many addresses it does not control. A classic account also needs a
    /// trustline for the token, as for any Stellar asset.
    pub fn drip(env: Env, to: Address) -> Result<i128, FaucetError> {
        to.require_auth();
        let config = Self::read_config(&env)?;

        let now = env.ledger().timestamp();
        let key = DataKey::LastDrip(to.clone());
        if let Some(last) = env.storage().temporary().get::<_, u64>(&key) {
            if now < last.saturating_add(config.cooldown) {
                return Err(FaucetError::CoolingDown);
            }
        }

        token::StellarAssetClient::new(&env, &config.token).mint(&to, &config.amount);

        env.storage().temporary().set(&key, &now);
        env.storage()
            .temporary()
            .extend_ttl(&key, DRIP_TTL_THRESHOLD, DRIP_TTL_EXTEND);
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

    // ----- internal -----

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
