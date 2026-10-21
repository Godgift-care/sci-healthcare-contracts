#![cfg(test)]
extern crate std;

use soroban_sdk::xdr::{ContractEvent, ContractEventBody, ScVal};
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    token, Address, Env, String,
};

use crate::{Faucet, FaucetClient, FaucetError};

const DRIP: i128 = 500_000_000; // 50 USDC at 7 decimals
const COOLDOWN: u64 = 24 * 60 * 60;
/// Two drips per day across every address combined.
const DAILY_CAP: i128 = 2 * DRIP;
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
    faucet.initialize(&admin, &token_addr, &DRIP, &COOLDOWN, &DAILY_CAP);

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
fn daily_cap_limits_total_minted_per_day() {
    let f = setup();
    let a = Address::generate(&f.env);
    let b = Address::generate(&f.env);
    let c = Address::generate(&f.env);

    f.faucet.drip(&a);
    f.faucet.drip(&b);
    assert_eq!(f.token.balance(&a), DRIP);
    assert_eq!(f.token.balance(&b), DRIP);

    // DAILY_CAP is two drips: a third address is rejected even though it has
    // never dripped before.
    let err = f.faucet.try_drip(&c).err().unwrap().unwrap();
    assert_eq!(err, FaucetError::DailyCapReached);
    assert_eq!(f.token.balance(&c), 0);
}

#[test]
fn daily_cap_resets_at_day_boundary() {
    let f = setup();
    let user = Address::generate(&f.env);
    let other = Address::generate(&f.env);

    f.faucet.drip(&user);
    f.faucet.drip(&other); // shared cap of two drips reached
    assert!(f.faucet.try_drip(&user).is_err());
    assert!(f.faucet.try_drip(&other).is_err());

    // The next UTC day: the shared counter resets and drips resume.
    advance(&f.env, 24 * 60 * 60 + 1);
    f.faucet.drip(&user);
    assert_eq!(f.token.balance(&user), 2 * DRIP);
    assert_eq!(f.token.balance(&other), DRIP);
}

#[test]
fn paused_faucet_rejects_drips() {
    let f = setup();
    let user = Address::generate(&f.env);

    f.faucet.set_paused(&f.admin, &true);
    let err = f.faucet.try_drip(&user).err().unwrap().unwrap();
    assert_eq!(err, FaucetError::Paused);

    f.faucet.set_paused(&f.admin, &false);
    f.faucet.drip(&user);
    assert_eq!(f.token.balance(&user), DRIP);
}

#[test]
fn non_admin_cannot_pause() {
    let f = setup();
    let impostor = Address::generate(&f.env);
    let err = f
        .faucet
        .try_set_paused(&impostor, &true)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, FaucetError::NotAuthorized);
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
fn version_is_the_crate_version() {
    let f = setup();
    let version = f.faucet.version();
    assert_eq!(version, String::from_str(&f.env, env!("CARGO_PKG_VERSION")));
    // Every contract shares the workspace version.
    assert_eq!(version, String::from_str(&f.env, "0.2.0"));
}

#[test]
fn initialize_is_single_shot() {
    let f = setup();
    let a = Address::generate(&f.env);
    let err = f
        .faucet
        .try_initialize(&a, &a, &DRIP, &COOLDOWN, &DAILY_CAP)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, FaucetError::AlreadyInitialized);
}

#[test]
fn zero_daily_cap_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let issuer = Address::generate(&env);
    let admin = Address::generate(&env);
    let asset = env.register_stellar_asset_contract_v2(issuer.clone());
    let token_addr = asset.address();
    let faucet_addr = env.register(Faucet, ());
    let faucet = FaucetClient::new(&env, &faucet_addr);

    let err = faucet
        .try_initialize(&admin, &token_addr, &DRIP, &COOLDOWN, &0i128)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, FaucetError::InvalidAmount);
}

// ----- event assertions -----

fn symbol_of(v: &ScVal) -> Option<&str> {
    match v {
        ScVal::Symbol(s) => (&s.0).try_into().ok(),
        _ => None,
    }
}

/// The events published by `contract` so far.
fn faucet_events(env: &Env, contract: &Address) -> std::vec::Vec<ContractEvent> {
    let events = env.events().all().filter_by_contract(contract);
    std::vec::Vec::from(events.events())
}

/// The name carried in topic 0 of the last faucet event.
fn last_event_name(env: &Env, contract: &Address) -> std::string::String {
    let list = faucet_events(env, contract);
    let last = list.last().unwrap();
    match &last.body {
        ContractEventBody::V0(v0) => match &v0.topics[0] {
            ScVal::Symbol(s) => std::string::String::from_utf8_lossy(&s.0).into_owned(),
            other => panic!("event topic 0 is not a symbol: {other:?}"),
        },
    }
}

/// One data-map field of the last faucet event.
fn last_event_field(env: &Env, contract: &Address, name: &str) -> ScVal {
    let list = faucet_events(env, contract);
    let last = list.last().unwrap();
    match &last.body {
        ContractEventBody::V0(v0) => match &v0.data {
            ScVal::Map(Some(map)) => map
                .iter()
                .find(|entry| symbol_of(&entry.key) == Some(name))
                .map(|entry| entry.val.clone())
                .unwrap_or_else(|| panic!("last event has no `{name}` field")),
            _ => panic!("last event data is not a map"),
        },
    }
}

fn last_event_i128(env: &Env, contract: &Address, name: &str) -> i128 {
    match last_event_field(env, contract, name) {
        ScVal::I128(parts) => i128::from(&parts),
        other => panic!("event field `{name}` is {other:?}, want i128"),
    }
}

fn last_event_u64(env: &Env, contract: &Address, name: &str) -> u64 {
    match last_event_field(env, contract, name) {
        ScVal::U64(v) => v,
        other => panic!("event field `{name}` is {other:?}, want u64"),
    }
}

#[test]
fn set_drip_emits_drip_changed_event() {
    let f = setup();
    let contract = &f.faucet.address;
    f.faucet.set_drip(&f.admin, &10i128, &0u64);

    let events = faucet_events(&f.env, contract);
    assert_eq!(events.len(), 1, "set_drip must publish exactly one event");
    assert_eq!(last_event_name(&f.env, contract), "drip_changed");
    assert_eq!(last_event_i128(&f.env, contract, "amount"), 10);
    assert_eq!(last_event_u64(&f.env, contract, "cooldown"), 0);
}

#[test]
fn not_initialized_errors() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = START_TIME);
    let a = Address::generate(&env);
    let issuer = Address::generate(&env);
    let asset = env.register_stellar_asset_contract_v2(issuer);
    let _token_addr = asset.address();
    let faucet_addr = env.register(Faucet, ());
    let faucet = FaucetClient::new(&env, &faucet_addr);

    // Any call before initialize should return NotInitialized.
    let err = faucet.try_get_config().err().unwrap().unwrap();
    assert_eq!(err, FaucetError::NotInitialized);
    let err = faucet.try_drip(&a).err().unwrap().unwrap();
    assert_eq!(err, FaucetError::NotInitialized);
}
