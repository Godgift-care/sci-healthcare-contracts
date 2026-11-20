#![cfg(test)]
extern crate std;

use std::string::ToString;

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
    setup_with_caps(DAILY_CAP, 0)
}

fn setup_with_caps(daily_cap: i128, lifetime_cap: i128) -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = START_TIME);

    let issuer = Address::generate(&env);
    let admin = Address::generate(&env);
    let asset = env.register_stellar_asset_contract_v2(issuer.clone());
    let token_addr = asset.address();

    let faucet_addr = env.register(Faucet, ());
    let faucet = FaucetClient::new(&env, &faucet_addr);
    faucet.initialize(
        &admin,
        &token_addr,
        &DRIP,
        &COOLDOWN,
        &daily_cap,
        &lifetime_cap,
    );

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
        .try_initialize(&a, &a, &DRIP, &COOLDOWN, &DAILY_CAP, &0i128)
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
        .try_initialize(&admin, &token_addr, &DRIP, &COOLDOWN, &0i128, &0i128)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, FaucetError::InvalidAmount);
}

#[test]
fn lifetime_cap_is_enforced_across_days() {
    // Two drips per address, ever, with room in the shared daily cap.
    let f = setup_with_caps(10 * DRIP, 2 * DRIP);
    let user = Address::generate(&f.env);

    f.faucet.drip(&user);
    advance(&f.env, COOLDOWN);
    f.faucet.drip(&user);

    // Another day and another cooldown do not reset a lifetime cap.
    advance(&f.env, 24 * 60 * 60 + 1);
    let err = f.faucet.try_drip(&user).err().unwrap().unwrap();
    assert_eq!(err, FaucetError::LifetimeCapReached);
    assert_eq!(f.token.balance(&user), 2 * DRIP);
}

#[test]
fn lifetime_cap_is_per_address() {
    let f = setup_with_caps(10 * DRIP, DRIP);
    let capped = Address::generate(&f.env);
    let other = Address::generate(&f.env);

    f.faucet.drip(&capped);
    advance(&f.env, COOLDOWN);
    let err = f.faucet.try_drip(&capped).err().unwrap().unwrap();
    assert_eq!(err, FaucetError::LifetimeCapReached);

    // A different address still has its own allowance.
    assert_eq!(f.faucet.drip(&other), DRIP);
    assert_eq!(f.token.balance(&other), DRIP);
}

#[test]
fn zero_lifetime_cap_means_unlimited() {
    let f = setup_with_caps(10 * DRIP, 0);
    let user = Address::generate(&f.env);

    for _ in 0..5 {
        f.faucet.drip(&user);
        advance(&f.env, COOLDOWN);
    }
    assert_eq!(f.token.balance(&user), 5 * DRIP);
}

#[test]
fn negative_lifetime_cap_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let issuer = Address::generate(&env);
    let admin = Address::generate(&env);
    let asset = env.register_stellar_asset_contract_v2(issuer);
    let token_addr = asset.address();
    let faucet_addr = env.register(Faucet, ());
    let faucet = FaucetClient::new(&env, &faucet_addr);

    let err = faucet
        .try_initialize(&admin, &token_addr, &DRIP, &COOLDOWN, &DAILY_CAP, &-1i128)
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

// ----- authorisation matrix -----
//
// Every public function is exercised in the state that makes it a valid call,
// and the test asserts who the host actually demanded. A missing require_auth
// lets anyone act for another party; an extra one makes a call need a
// signature nobody intended to collect.
//
// The table in `docs/developers/contracts.md` is the rendered form of this
// same list, and `authorisation_table_in_the_docs_matches_the_matrix` fails
// when the two drift.

/// A plain fixture plus the addresses the matrix rows need. They are generated
/// here rather than in `setup()` so every other test keeps the same generated
/// addresses — and therefore the same snapshots.
struct AuthFixture {
    f: Fixture,
    user: Address,
    new_admin: Address,
}

impl AuthFixture {
    fn new() -> Self {
        let f = setup();
        let user = Address::generate(&f.env);
        let new_admin = Address::generate(&f.env);
        Self { f, user, new_admin }
    }
}

#[derive(Clone, Copy)]
struct Signer {
    label: &'static str,
    address: fn(&AuthFixture) -> Address,
}

#[derive(Clone, Copy)]
enum Auth {
    Signer(Signer),
    Free(&'static str),
}

impl Auth {
    fn label(self) -> &'static str {
        match self {
            Auth::Signer(signer) => signer.label,
            Auth::Free(note) => note,
        }
    }
}

const fn signer(label: &'static str, address: fn(&AuthFixture) -> Address) -> Auth {
    Auth::Signer(Signer { label, address })
}

struct AuthCase {
    function: &'static str,
    auth: Auth,
}

fn admin_addr(f: &AuthFixture) -> Address {
    f.f.admin.clone()
}

fn user_addr(f: &AuthFixture) -> Address {
    f.user.clone()
}

/// Every public function of the faucet, in declaration order.
const AUTH_MATRIX: &[AuthCase] = &[
    AuthCase {
        function: "initialize",
        auth: Auth::Free("none (one-shot)"),
    },
    AuthCase {
        function: "drip",
        auth: signer("`to`", user_addr),
    },
    AuthCase {
        function: "set_drip",
        auth: signer("`admin`", admin_addr),
    },
    AuthCase {
        function: "set_paused",
        auth: signer("`admin`", admin_addr),
    },
    AuthCase {
        function: "release_token_admin",
        auth: signer("`admin`", admin_addr),
    },
    AuthCase {
        function: "get_config",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "next_drip_at",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "version",
        auth: Auth::Free("none (view)"),
    },
];

fn recorded_auths(env: &Env) -> std::vec::Vec<Address> {
    env.auths()
        .into_iter()
        .map(|(address, _)| address)
        .collect()
}

/// What one successful call demanded and touched.
struct CaseRun {
    auths: std::vec::Vec<Address>,
    /// In-memory ledger entries read by the last top-level invocation.
    reads: u32,
    /// Ledger entries written by the last top-level invocation.
    writes: u32,
}

fn case_run(env: &Env) -> CaseRun {
    let resources = env.cost_estimate().resources();
    CaseRun {
        auths: recorded_auths(env),
        reads: resources.memory_read_entries,
        writes: resources.write_entries,
    }
}

fn run_case(f: &AuthFixture, function: &str) -> CaseRun {
    let faucet = &f.f.faucet;
    let env = &f.f.env;

    match function {
        // One-shot setup needs a fresh contract, so it cannot share the
        // fixture: assert on an env that has never been initialised.
        "initialize" => {
            let env = Env::default();
            env.mock_all_auths();
            let admin = Address::generate(&env);
            let issuer = Address::generate(&env);
            let asset = env.register_stellar_asset_contract_v2(issuer);
            let address = env.register(Faucet, ());
            FaucetClient::new(&env, &address).initialize(
                &admin,
                &asset.address(),
                &DRIP,
                &COOLDOWN,
                &DAILY_CAP,
                &0i128,
            );
            return case_run(&env);
        }
        "drip" => {
            faucet.drip(&f.user);
        }
        "set_drip" => {
            faucet.set_drip(&f.f.admin, &DRIP, &COOLDOWN);
        }
        "set_paused" => {
            faucet.set_paused(&f.f.admin, &true);
        }
        "release_token_admin" => {
            faucet.release_token_admin(&f.f.admin, &f.new_admin);
        }
        "get_config" => {
            faucet.get_config();
        }
        "next_drip_at" => {
            faucet.next_drip_at(&f.user);
        }
        "version" => {
            faucet.version();
        }
        other => {
            panic!("no call is defined for `{other}`, so the matrix test cannot check it")
        }
    }

    case_run(env)
}

fn assert_demands(f: &AuthFixture, case: &AuthCase, got: &[Address]) {
    match case.auth {
        Auth::Signer(signer) => {
            let expected = (signer.address)(f);
            assert_eq!(
                got.len(),
                1,
                "`{}` must demand exactly one signer ({}), but the host recorded {}: \
                 a missing require_auth() lets anyone drain the faucet or change its \
                 settings, and an extra one breaks callers that expect the faucet to \
                 be permissionless",
                case.function,
                signer.label,
                got.len(),
            );
            assert_eq!(
                got[0], expected,
                "`{}` demands a different signer than the documented {}. \
                 Changing this line without changing the contract means the contract \
                 is not checking who it claims to check",
                case.function, signer.label,
            );
        }
        Auth::Free(note) => assert!(
            got.is_empty(),
            "`{}` is documented as {note}, but it demanded {} signature(s). \
             If that is intentional, every caller has to change",
            case.function,
            got.len(),
        ),
    }
}

#[test]
fn authorisation_matrix_matches_every_public_function() {
    for case in AUTH_MATRIX {
        let f = AuthFixture::new();
        let run = run_case(&f, case.function);
        assert_demands(&f, case, &run.auths);
    }
}

fn doc_rows<'a>(docs: &'a str, contract: &str) -> std::vec::Vec<&'a str> {
    let heading = if contract == "faucet" {
        "## faucet (testnet only)\n".to_string()
    } else {
        std::format!("## {contract}\n")
    };
    let start = docs
        .find(&heading)
        .unwrap_or_else(|| panic!("contracts.md has no `{contract}` section"));
    let section = &docs[start..];
    let start = section
        .find("### Authorisation matrix")
        .unwrap_or_else(|| panic!("the `{contract}` section has no authorisation matrix"));
    let section = &section[start..];
    let end = find_section_end(section);
    section[..end]
        .lines()
        .filter(|line| line.starts_with("| `"))
        .collect()
}

fn find_section_end(section: &str) -> usize {
    let mut end = section.len();
    if let Some(pos) = section.find("\n### ") {
        end = end.min(pos);
    }
    if let Some(pos) = section.find("\n## ") {
        end = end.min(pos);
    }
    if let Some(pos) = section.find("\n---") {
        end = end.min(pos);
    }
    end
}

#[test]
fn authorisation_table_in_the_docs_matches_the_matrix() {
    const DOCS: &str = include_str!("../../../docs/developers/contracts.md");

    let rows = doc_rows(DOCS, "faucet");
    assert_eq!(
        rows.len(),
        AUTH_MATRIX.len(),
        "docs/developers/contracts.md documents {} faucet functions but the matrix has {}",
        rows.len(),
        AUTH_MATRIX.len(),
    );
    for case in AUTH_MATRIX {
        let want = std::format!("| `{}` | {} |", case.function, case.auth.label());
        assert!(
            rows.contains(&want.as_str()),
            "docs/developers/contracts.md is missing the row `{want}`. \
             Regenerate the table from AUTH_MATRIX so the documented signer is the \
             one the tests prove",
        );
    }
}

// ----- storage read and write counts -----
//
// Entry counts drive fees and the resource limits a transaction can hit. This
// table is generated by running every row of `AUTH_MATRIX` once in the state
// that makes the call valid, and recording the host's metered ledger entries.
//
// The test fails when a count changes: an increase is a cost regression, and a
// decrease still means the committed table is stale. Regenerate intentionally
// with `just storage-counts` (or `./scripts/refresh-storage-counts.sh`) and
// explain the change in the pull request.
//
// What breaks if this is wrong: a new storage read or write in a hot path is
// invisible until a real transaction hits the network's entry limit.

const STORAGE_COUNTS_FILE: &str = "storage-counts.md";

fn storage_counts_table() -> std::string::String {
    let mut table = std::string::String::from(
        "# Storage reads and writes per public entry point\n\
         \n\
         Generated by `just storage-counts` from `AUTH_MATRIX` in this crate. \
         Reads are `memory_read_entries` and writes are `write_entries` from \
         `Env::cost_estimate().resources()` after one successful call. Update \
         the table in the same pull request as an intentional change.\n\
         \n\
         | Function | Reads | Writes |\n\
         | --- | --- | --- |\n",
    );
    for case in AUTH_MATRIX {
        let f = AuthFixture::new();
        let run = run_case(&f, case.function);
        table.push_str(&std::format!(
            "| `{}` | {} | {} |\n",
            case.function,
            run.reads,
            run.writes,
        ));
    }
    table
}

#[test]
fn storage_counts_match_the_committed_table() {
    let table = storage_counts_table();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(STORAGE_COUNTS_FILE);
    if std::env::var_os("UPDATE_STORAGE_COUNTS").is_some() {
        std::fs::write(&path, &table).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "{} is missing; regenerate it with `just storage-counts`",
            path.display()
        )
    });
    assert_eq!(
        committed, table,
        "storage reads or writes changed for a faucet entry point. If the change is \
         intentional, run `just storage-counts` and explain the new cost in the pull \
         request; if it is not, the call is doing more ledger work than it used to",
    );
}

// ----- event table in the docs -----
//
// Events are the indexer's API. This test reads the `#[contractevent]` structs
// in `lib.rs` and compares the event names, topics and data fields with the
// table in `docs/developers/contracts.md`. Adding a field without documenting
// it fails here rather than in a downstream decoder.

#[derive(Debug)]
struct SourceEvent {
    name: std::string::String,
    topics: std::vec::Vec<std::string::String>,
    data: std::vec::Vec<std::string::String>,
}

fn snake_case(name: &str) -> std::string::String {
    let mut out = std::string::String::new();
    for (index, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if index != 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

fn events_in_source(source: &str) -> std::vec::Vec<SourceEvent> {
    let mut events = std::vec::Vec::new();
    let mut seen_attribute = false;
    let mut current: Option<SourceEvent> = None;
    let mut pending_topic = false;

    for raw in source.lines() {
        let line = raw.trim();
        if line.starts_with("#[contractevent") {
            seen_attribute = true;
            continue;
        }
        let Some(event) = current.as_mut() else {
            if seen_attribute {
                if let Some(rest) = line.strip_prefix("pub struct ") {
                    let name = rest.trim().trim_end_matches('{').trim();
                    current = Some(SourceEvent {
                        name: snake_case(name),
                        topics: std::vec::Vec::new(),
                        data: std::vec::Vec::new(),
                    });
                    seen_attribute = false;
                }
            }
            continue;
        };
        if line == "}" {
            events.push(current.take().unwrap());
            continue;
        }
        if line == "#[topic]" {
            pending_topic = true;
            continue;
        }
        if let Some(rest) = line.strip_prefix("pub ") {
            let field = rest.split(':').next().unwrap().trim();
            if pending_topic {
                event.topics.push(field.into());
            } else {
                event.data.push(field.into());
            }
            pending_topic = false;
        }
    }

    events
}

fn doc_event_rows<'a>(docs: &'a str, contract: &str) -> std::vec::Vec<&'a str> {
    let heading = if contract == "faucet" {
        "## faucet (testnet only)\n".to_string()
    } else {
        std::format!("## {contract}\n")
    };
    let start = docs
        .find(&heading)
        .unwrap_or_else(|| panic!("contracts.md has no `{contract}` section"));
    let section = &docs[start..];
    let start = section
        .find("### Events")
        .unwrap_or_else(|| panic!("the `{contract}` section has no event table"));
    let section = &section[start..];
    let end = find_section_end(section);
    section[..end]
        .lines()
        .filter(|line| line.starts_with("| `"))
        .collect()
}

fn split_doc_fields(cell: &str) -> std::vec::Vec<std::string::String> {
    let trimmed = cell.trim();
    if trimmed.is_empty() || trimmed == "-" || trimmed == "—" {
        return std::vec::Vec::new();
    }
    cell.split(',')
        .map(|field| field.trim().trim_matches('`').into())
        .collect()
}

fn doc_event_row(line: &str) -> std::vec::Vec<std::string::String> {
    line.trim()
        .trim_matches('|')
        .split('|')
        .map(|cell| cell.trim().trim_matches('`').into())
        .collect()
}

#[test]
fn event_table_in_the_docs_matches_the_source() {
    const DOCS: &str = include_str!("../../../docs/developers/contracts.md");
    const SOURCE: &str = include_str!("lib.rs");

    let source = events_in_source(SOURCE);
    let rows = doc_event_rows(DOCS, "faucet");
    assert_eq!(
        rows.len(),
        source.len(),
        "docs/developers/contracts.md documents {} faucet events but the source has {}",
        rows.len(),
        source.len(),
    );
    for event in source {
        let row = rows
            .iter()
            .map(|line| doc_event_row(line))
            .find(|row| row[0] == event.name)
            .unwrap_or_else(|| {
                panic!(
                    "docs/developers/contracts.md is missing the `{}` event. Add its row \
                     to the faucet event table with topics {:?} and data {:?}",
                    event.name, event.topics, event.data,
                )
            });
        assert_eq!(
            split_doc_fields(&row[1]),
            event.topics,
            "the topics documented for `{}` do not match the #[topic] fields in \
             contracts/faucet/src/lib.rs",
            event.name,
        );
        assert_eq!(
            split_doc_fields(&row[2]),
            event.data,
            "the data fields documented for `{}` do not match the non-topic fields in \
             contracts/faucet/src/lib.rs. The indexer decodes them by name \
             (TOPIC_FIELDS in sci-healthcare-backend/src/stellar/events.ts)",
            event.name,
        );
    }
}

// ----- stored-type upgrade safety -----
//
// The fixtures in `fixtures/stored_types/` are XDR of the values the latest
// release writes. If a change to a `#[contracttype]` struct makes an old
// fixture undecodable, every entry written by the previous release is
// unreadable unless the change is handled deliberately. Regenerate only at
// release time with `just stored-type-fixtures`.

fn fixture_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/stored_types")
        .join(std::format!("{name}.hex"))
}

fn to_hex(bytes: &soroban_sdk::Bytes) -> std::string::String {
    use std::fmt::Write as _;

    let mut out = std::string::String::new();
    for byte in bytes.iter() {
        write!(&mut out, "{byte:02x}").unwrap();
    }
    out
}

fn from_hex(env: &Env, text: &str) -> soroban_sdk::Bytes {
    let text = text.trim();
    assert_eq!(
        text.len() % 2,
        0,
        "a stored-type fixture has an odd hex length"
    );
    let mut bytes = std::vec::Vec::with_capacity(text.len() / 2);
    for index in (0..text.len()).step_by(2) {
        bytes.push(u8::from_str_radix(&text[index..index + 2], 16).unwrap());
    }
    soroban_sdk::Bytes::from_slice(env, &bytes)
}

fn check_stored_type<T>(env: &Env, name: &str, expected: T)
where
    T: soroban_sdk::xdr::ToXdr + soroban_sdk::xdr::FromXdr + Clone + PartialEq + std::fmt::Debug,
    <T as soroban_sdk::xdr::FromXdr>::Error: std::fmt::Debug,
{
    let bytes = expected.clone().to_xdr(env);
    let path = fixture_path(name);

    if std::env::var_os("UPDATE_STORED_TYPE_FIXTURES").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, std::format!("{}\n", to_hex(&bytes))).unwrap();
        return;
    }

    let committed = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "{} is missing; regenerate fixtures with `just stored-type-fixtures`",
            path.display()
        )
    });
    let fixture = from_hex(env, &committed);
    match T::from_xdr(env, &fixture) {
        Ok(decoded) => assert_eq!(
            decoded, expected,
            "the stored `{name}` fixture decodes to a different value than the current \
             code writes. This is a breaking storage change: keep the old layout, put \
             new fields under a new data key, or add a versioned migration before \
             refreshing the fixture",
        ),
        Err(error) => panic!(
            "the stored `{name}` fixture no longer decodes into the current type: \
             {error:?}. Adding, removing, renaming or reordering a field is a breaking \
             storage change. Keep the old layout, put new fields under a new data key \
             with a default, or add a versioned migration; only then refresh fixtures \
             with `just stored-type-fixtures`",
        ),
    }
}

#[test]
fn stored_type_fixtures_decode_against_current_code() {
    use crate::FaucetConfig;

    let env = Env::default();
    let admin = Address::from_str(
        &env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    );
    let token = Address::from_str(
        &env,
        "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4",
    );

    check_stored_type(
        &env,
        "faucet_config",
        FaucetConfig {
            admin,
            token,
            amount: DRIP,
            cooldown: COOLDOWN,
            daily_cap: DAILY_CAP,
            lifetime_cap: 0,
            paused: false,
        },
    );
}
