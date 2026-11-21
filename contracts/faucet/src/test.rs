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
    setup_full(COOLDOWN, daily_cap, lifetime_cap)
}

fn setup_full(cooldown: u64, daily_cap: i128, lifetime_cap: i128) -> Fixture {
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
        &cooldown,
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

// ----- many addresses, day boundaries and cap orderings (#299) -----
//
// The daily cap is shared by every address, so the interesting cases are
// several addresses hitting it together, around midnight UTC, while some of
// them also run into their lifetime cap. Each test below drives ten
// addresses through a schedule and compares every drip with a small
// reference model of the rules (cooldown, then lifetime cap, then the shared
// daily cap), asserting after every call that nothing minted more than the
// caps allow.
//
// What breaks if this is wrong: the faucet mints more test USDC in a day
// than its cap — or one address more than its lifetime allowance — when
// many addresses drip in an unlucky order, which is exactly what a script
// draining the faucet would do.

const ADDRESSES: usize = 10;
const SECONDS_PER_DAY: u64 = 24 * 60 * 60;
/// Spacing of the repeated rounds of attempts in the longer schedules.
const ROUND: u64 = 4 * 60 * 60;
const ROUNDS_PER_DAY: u64 = SECONDS_PER_DAY / ROUND;
/// The first midnight UTC after `START_TIME`.
const MIDNIGHT: u64 = (START_TIME / SECONDS_PER_DAY + 1) * SECONDS_PER_DAY;

fn addresses(env: &Env) -> std::vec::Vec<Address> {
    (0..ADDRESSES).map(|_| Address::generate(env)).collect()
}

/// The faucet's rules, written out independently of the contract.
struct Model {
    cooldown: u64,
    daily_cap: i128,
    lifetime_cap: i128,
    last: std::vec::Vec<Option<u64>>,
    lifetime: std::vec::Vec<i128>,
    /// Minted per UTC day, keyed by day number.
    per_day: std::collections::BTreeMap<u64, i128>,
}

impl Model {
    fn new(cooldown: u64, daily_cap: i128, lifetime_cap: i128) -> Self {
        Model {
            cooldown,
            daily_cap,
            lifetime_cap,
            last: std::vec![None; ADDRESSES],
            lifetime: std::vec![0; ADDRESSES],
            per_day: std::collections::BTreeMap::new(),
        }
    }

    fn drip(&mut self, who: usize, now: u64) -> Result<i128, FaucetError> {
        if let Some(t) = self.last[who] {
            if now < t + self.cooldown {
                return Err(FaucetError::CoolingDown);
            }
        }
        if self.lifetime_cap > 0 && self.lifetime[who] + DRIP > self.lifetime_cap {
            return Err(FaucetError::LifetimeCapReached);
        }
        let day = now / SECONDS_PER_DAY;
        let today = self.per_day.get(&day).copied().unwrap_or(0);
        if today + DRIP > self.daily_cap {
            return Err(FaucetError::DailyCapReached);
        }
        self.last[who] = Some(now);
        self.lifetime[who] += DRIP;
        self.per_day.insert(day, today + DRIP);
        Ok(DRIP)
    }

    fn total(&self) -> i128 {
        self.lifetime.iter().sum()
    }
}

/// One step of a schedule: move the clock forward, then `who` drips.
type Step = (u64, usize);

/// Runs `steps` against both the contract and the model, checking every
/// outcome matches and the caps hold after every call. Returns the model.
fn run_schedule(f: &Fixture, model: &mut Model, users: &[Address], steps: &[Step]) {
    for (i, &(wait, who)) in steps.iter().enumerate() {
        advance(&f.env, wait);
        let now = f.env.ledger().timestamp();
        let expected = model.drip(who, now);
        let got = match f.faucet.try_drip(&users[who]) {
            Ok(Ok(amount)) => Ok(amount),
            Err(Ok(e)) => Err(e),
            other => panic!("step {i}: unexpected result {other:?}"),
        };
        assert_eq!(got, expected, "step {i}: address {who} at t={now}");

        // Invariants, after every single call: the real balances match the
        // model, no address is over its lifetime cap and no day is over the
        // shared cap.
        let mut minted = 0;
        for (idx, u) in users.iter().enumerate() {
            let bal = f.token.balance(u);
            assert_eq!(bal, model.lifetime[idx], "step {i}: balance of {idx}");
            if model.lifetime_cap > 0 {
                assert!(
                    bal <= model.lifetime_cap,
                    "step {i}: {idx} over lifetime cap"
                );
            }
            minted += bal;
        }
        assert_eq!(minted, model.total(), "step {i}: minted total drifted");
        for (day, amount) in &model.per_day {
            assert!(
                *amount <= model.daily_cap,
                "step {i}: day {day} minted {amount} over the cap"
            );
        }
    }
}

/// The orderings exercised: forward, reverse, evens then odds, and a few
/// deterministic shuffles.
fn orderings() -> std::vec::Vec<std::vec::Vec<usize>> {
    let mut out = std::vec![
        (0..ADDRESSES).collect::<std::vec::Vec<_>>(),
        (0..ADDRESSES).rev().collect(),
        (0..ADDRESSES)
            .step_by(2)
            .chain((1..ADDRESSES).step_by(2))
            .collect(),
    ];
    for seed in [7u64, 42, 1_234_567] {
        // Fisher–Yates with a small LCG: reproducible, no extra crates.
        let mut order: std::vec::Vec<usize> = (0..ADDRESSES).collect();
        let mut x = seed;
        for i in (1..ADDRESSES).rev() {
            x = x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let j = (x >> 33) as usize % (i + 1);
            order.swap(i, j);
        }
        out.push(order);
    }
    out
}

#[test]
fn ten_addresses_exhaust_the_shared_daily_cap() {
    let daily_cap = 4 * DRIP;
    for order in orderings() {
        let f = setup_full(COOLDOWN, daily_cap, 0);
        let users = addresses(&f.env);
        let mut model = Model::new(COOLDOWN, daily_cap, 0);

        let steps: std::vec::Vec<Step> = order.iter().map(|&who| (0, who)).collect();
        run_schedule(&f, &mut model, &users, &steps);

        // Exactly four drips went through, whoever came first.
        let minted: i128 = users.iter().map(|u| f.token.balance(u)).sum();
        assert_eq!(minted, daily_cap, "order {order:?}");
        for (pos, &who) in order.iter().enumerate() {
            let expected = if pos < 4 { DRIP } else { 0 };
            assert_eq!(f.token.balance(&users[who]), expected, "order {order:?}");
        }

        // A drip refused by the daily cap does not start a cooldown.
        for &who in &order[4..] {
            assert_eq!(f.faucet.next_drip_at(&users[who]), 0);
        }
    }
}

#[test]
fn daily_cap_resets_at_midnight_not_after_a_day() {
    // Start one minute before midnight UTC. The cap is used up, then resets
    // a minute later at the day boundary — not 24 hours after the first
    // drip — while the 24-hour cooldown still holds the earlier drippers.
    let daily_cap = 4 * DRIP;
    for order in orderings() {
        let f = setup_full(COOLDOWN, daily_cap, 0);
        f.env.ledger().with_mut(|li| li.timestamp = MIDNIGHT - 60);
        let users = addresses(&f.env);
        let mut model = Model::new(COOLDOWN, daily_cap, 0);

        // Everyone tries before midnight: four succeed.
        let mut steps: std::vec::Vec<Step> = order.iter().map(|&who| (0, who)).collect();
        // Midnight passes; everyone tries again in the same order. The four
        // earlier drippers are cooling down; the next four succeed.
        steps.push((60, order[0]));
        steps.extend(order[1..].iter().map(|&who| (0, who)));
        run_schedule(&f, &mut model, &users, &steps);

        let day_before = (MIDNIGHT - 60) / SECONDS_PER_DAY;
        assert_eq!(model.per_day[&day_before], daily_cap);
        assert_eq!(model.per_day[&(day_before + 1)], daily_cap);
        let minted: i128 = users.iter().map(|u| f.token.balance(u)).sum();
        assert_eq!(minted, 2 * daily_cap, "order {order:?}");
        // Two addresses never got a drip: the cap, not the cooldown, stopped them.
        for &who in &order[8..] {
            assert_eq!(f.token.balance(&users[who]), 0);
        }
    }
}

#[test]
fn frequent_attempts_across_three_days_never_exceed_the_daily_cap() {
    // A short cooldown makes the daily cap the only brake: ten addresses try
    // every four hours for three days, starting mid-afternoon so two
    // midnights fall inside the run.
    let cooldown = ROUND;
    let daily_cap = 7 * DRIP;
    for order in orderings() {
        let f = setup_full(cooldown, daily_cap, 0);
        f.env
            .ledger()
            .with_mut(|li| li.timestamp = MIDNIGHT - 9 * 60 * 60);
        let users = addresses(&f.env);
        let mut model = Model::new(cooldown, daily_cap, 0);

        let mut steps = std::vec::Vec::new();
        for round in 0..(3 * ROUNDS_PER_DAY) {
            for (pos, &who) in order.iter().enumerate() {
                let wait = if pos == 0 && round > 0 { ROUND - 9 } else { 1 };
                steps.push((wait, who));
            }
        }
        run_schedule(&f, &mut model, &users, &steps);

        // Every full day minted exactly the cap; the partial ones no more.
        let days: std::vec::Vec<_> = model.per_day.iter().collect();
        assert!(days.len() >= 3, "the run must cross midnight twice");
        for (_, minted) in &days[1..days.len() - 1] {
            assert_eq!(**minted, daily_cap, "order {order:?}");
        }
    }
}

#[test]
fn lifetime_caps_hit_in_different_orders_never_overmint() {
    // Each address may receive two drips ever; the faucet may mint five a
    // day. Over five days of attempts every four hours in each ordering, the
    // daily cap runs out first, then lifetime caps take over, and the total
    // ends at exactly ten addresses × two drips.
    let cooldown = ROUND;
    let daily_cap = 5 * DRIP;
    let lifetime_cap = 2 * DRIP;
    for order in orderings() {
        let f = setup_full(cooldown, daily_cap, lifetime_cap);
        let users = addresses(&f.env);
        let mut model = Model::new(cooldown, daily_cap, lifetime_cap);

        let mut steps = std::vec::Vec::new();
        for round in 0..(5 * ROUNDS_PER_DAY as usize) {
            // Rotate the order each round so a different address leads.
            for k in 0..ADDRESSES {
                let who = order[(k + round) % ADDRESSES];
                let wait = if k == 0 && round > 0 { ROUND - 9 } else { 1 };
                steps.push((wait, who));
            }
        }
        run_schedule(&f, &mut model, &users, &steps);

        let minted: i128 = users.iter().map(|u| f.token.balance(u)).sum();
        assert_eq!(minted, ADDRESSES as i128 * lifetime_cap, "order {order:?}");
        for u in &users {
            assert_eq!(f.token.balance(u), lifetime_cap);
            let err = f.faucet.try_drip(u).err().unwrap().unwrap();
            assert!(
                err == FaucetError::LifetimeCapReached || err == FaucetError::CoolingDown,
                "an exhausted address must stay refused"
            );
        }
    }
}

#[test]
fn lifetime_cap_refusals_do_not_use_up_the_daily_cap() {
    // Five addresses reach their lifetime cap on day one. On day two they
    // keep trying first, and are refused; those refusals must not eat into
    // the shared daily cap, so the other five still get their drips.
    let cooldown = ROUND;
    let daily_cap = 5 * DRIP;
    let lifetime_cap = DRIP;
    let f = setup_full(cooldown, daily_cap, lifetime_cap);
    let users = addresses(&f.env);
    let mut model = Model::new(cooldown, daily_cap, lifetime_cap);

    let mut steps: std::vec::Vec<Step> = (0..5).map(|who| (0, who)).collect();
    steps.push((SECONDS_PER_DAY, 0));
    for _ in 0..3 {
        steps.extend((1..5).map(|who| (1, who)));
        steps.push((1, 0));
    }
    steps.extend((5..ADDRESSES).map(|who| (1, who)));
    run_schedule(&f, &mut model, &users, &steps);

    for u in &users {
        assert_eq!(f.token.balance(u), DRIP);
    }
}
