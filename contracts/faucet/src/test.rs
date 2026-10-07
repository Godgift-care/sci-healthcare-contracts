#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, Env,
};

use crate::{Faucet, FaucetClient, FaucetError};

const DRIP: i128 = 500_000_000; // 50 USDC at 7 decimals
const COOLDOWN: u64 = 24 * 60 * 60;
const START_TIME: u64 = 1_700_000_000;

struct Fixture {
    env: Env,
    faucet: FaucetClient<'static>,
    token: token::Client<'static>,
    sac: token::StellarAssetClient<'static>,
    admin: Address,
    issuer: Address,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = START_TIME);

    let issuer = Address::generate(&env);
    let admin = Address::generate(&env);
    let asset = env.register_stellar_asset_contract_v2(issuer.clone());
    let token_addr = asset.address();

    let faucet_addr = env.register(Faucet, ());
    let faucet = FaucetClient::new(&env, &faucet_addr);
    faucet.initialize(&admin, &token_addr, &DRIP, &COOLDOWN);

    // The issuer hands token administration to the faucet, as on testnet.
    let sac = token::StellarAssetClient::new(&env, &token_addr);
    sac.set_admin(&faucet_addr);

    Fixture {
        token: token::Client::new(&env, &token_addr),
        sac,
        env,
        faucet,
        admin,
        issuer,
    }
}

fn advance(env: &Env, secs: u64) {
    env.ledger().with_mut(|li| li.timestamp += secs);
}

#[test]
fn drip_mints_the_configured_amount() {
    let f = setup();
    let user = Address::generate(&f.env);

    assert_eq!(f.faucet.drip(&user), DRIP);
    assert_eq!(f.token.balance(&user), DRIP);
}

#[test]
fn drip_requires_the_recipients_signature() {
    let f = setup();
    let user = Address::generate(&f.env);
    f.faucet.drip(&user);

    let auths = f.env.auths();
    assert_eq!(auths.len(), 1);
    assert_eq!(auths[0].0, user);
}

#[test]
fn second_drip_waits_for_the_cooldown() {
    let f = setup();
    let user = Address::generate(&f.env);
    f.faucet.drip(&user);

    let err = f.faucet.try_drip(&user).err().unwrap().unwrap();
    assert_eq!(err, FaucetError::CoolingDown);
    assert_eq!(f.faucet.next_drip_at(&user), START_TIME + COOLDOWN);

    advance(&f.env, COOLDOWN);
    assert_eq!(f.faucet.next_drip_at(&user), 0);
    f.faucet.drip(&user);
    assert_eq!(f.token.balance(&user), 2 * DRIP);
}

#[test]
fn cooldown_is_per_address() {
    let f = setup();
    let a = Address::generate(&f.env);
    let b = Address::generate(&f.env);
    f.faucet.drip(&a);
    f.faucet.drip(&b);
    assert_eq!(f.token.balance(&b), DRIP);
}

#[test]
fn admin_can_change_the_drip() {
    let f = setup();
    f.faucet.set_drip(&f.admin, &10i128, &0u64);
    let user = Address::generate(&f.env);
    f.faucet.drip(&user);
    f.faucet.drip(&user); // zero cooldown
    assert_eq!(f.token.balance(&user), 20);
}

#[test]
fn non_admin_cannot_change_the_drip() {
    let f = setup();
    let someone = Address::generate(&f.env);
    let err = f
        .faucet
        .try_set_drip(&someone, &1i128, &0u64)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, FaucetError::NotAuthorized);
}

#[test]
fn zero_drip_is_rejected() {
    let f = setup();
    let err = f
        .faucet
        .try_set_drip(&f.admin, &0i128, &COOLDOWN)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, FaucetError::InvalidAmount);
}

#[test]
fn token_admin_can_be_handed_back() {
    let f = setup();
    f.faucet.release_token_admin(&f.admin, &f.issuer);
    assert_eq!(f.sac.admin(), f.issuer);

    // With admin rights gone, the faucet can no longer mint.
    let user = Address::generate(&f.env);
    assert!(f.faucet.try_drip(&user).is_err());
}

#[test]
fn non_admin_cannot_take_the_token() {
    let f = setup();
    let thief = Address::generate(&f.env);
    let err = f
        .faucet
        .try_release_token_admin(&thief, &thief)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, FaucetError::NotAuthorized);
}

#[test]
fn initialize_is_single_shot() {
    let f = setup();
    let a = Address::generate(&f.env);
    let err = f
        .faucet
        .try_initialize(&a, &a, &DRIP, &COOLDOWN)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, FaucetError::AlreadyInitialized);
}
