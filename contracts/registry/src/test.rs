#![cfg(test)]

use std::string::ToString;

use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, BytesN, Env, String, Vec,
};

use crate::{
    Provider, ProviderCategory, ProviderStatus, Registry, RegistryClient, RegistryError,
    ServiceItem, MAX_PRICE_JUMP, MAX_PROVIDERS_PAGE, MIN_PRICE_NOTICE_SECS,
};

mod admin;
mod attesters;
mod listing;
mod providers;
mod services;

/// Reason codes as documented in `docs/protocol/trust.md`: codes are added,
/// never renumbered. `1` is "verified" — the routine reason an admin
/// activates a freshly verified clinic.
const REASON_VERIFIED: u32 = 1;

/// A different code, so the event snapshot proves `reason_code` is what the
/// caller passed rather than a constant. `3` is "complaint under review".
const REASON_SUSPENDED_WHILE_TESTING: u32 = 3;

struct Fixture {
    env: Env,
    client: RegistryClient<'static>,
    admin: Address,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(Registry, ());
    let client = RegistryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    Fixture { env, client, admin }
}

/// Registers `provider` and activates it, so a call that needs an active
/// provider fails for authorisation reasons only.
fn activate(f: &Fixture, provider: &Address) {
    f.client.register_provider(
        provider,
        &String::from_str(&f.env, "Ikeja General Clinic"),
        &String::from_str(&f.env, "NG"),
        &ProviderCategory::Clinic,
    );
    f.client.set_provider_status(
        &f.admin,
        provider,
        &ProviderStatus::Active,
        &REASON_VERIFIED,
    );
}

fn register_active_provider(f: &Fixture) -> Address {
    let provider = Address::generate(&f.env);
    activate(f, &provider);
    provider
}

#[test]
fn initialize_sets_admin() {
    let f = setup();
    assert_eq!(f.client.get_admin(), f.admin);
}

#[test]
fn version_is_the_crate_version() {
    let f = setup();
    let version = f.client.version();
    assert_eq!(version, String::from_str(&f.env, env!("CARGO_PKG_VERSION")));
    // Every contract shares the workspace version.
    assert_eq!(version, String::from_str(&f.env, "0.2.0"));
}

#[test]
fn initialize_is_single_shot() {
    let f = setup();
    let other = Address::generate(&f.env);
    let err = f.client.try_initialize(&other).err().unwrap().unwrap();
    assert_eq!(err, RegistryError::AlreadyInitialized);
}

/// Lists one service at `PRICE` at a provider and starts the clock at
/// `START`, so schedules can be expressed relative to it.
fn priced_service(f: &Fixture) -> Address {
    f.env.ledger().with_mut(|li| li.timestamp = START);
    let provider = register_active_provider(f);
    f.client.upsert_service(
        &provider,
        &CONSULT,
        &String::from_str(&f.env, "Outpatient consult"),
        &PRICE,
    );
    provider
}

const START: u64 = 1_700_000_000;

// ----- authorisation matrix -----
//
// One row per public function: the exact signer it must demand, and nothing
// else. The host records one entry per address a `require_auth()` was made
// for during an invocation, so comparing `env.auths()` with the expected
// signer catches both kinds of mistake:
//
//   * a missing `require_auth()` — anyone can act for another party, and
//   * an extra `require_auth()` — every caller has to collect a signature
//     nobody needs, or the call cannot be made at all.
//
// The table in `docs/developers/contracts.md` is the rendered form of this
// same list, and `authorisation_table_in_the_docs_matches_the_matrix` fails
// when the two drift.
//
// What breaks if this is wrong: an authorisation hole here is the difference
// between "an admin activates a clinic" and "anyone activates any clinic".

extern crate std;

use soroban_sdk::testutils::storage::Persistent as _;
use soroban_sdk::testutils::Events as _;
use soroban_sdk::xdr::{ContractEvent, ContractEventBody, ScVal};

use crate::DataKey;

const CONSULT: u32 = 101;
const PRICE: i128 = 30_000_000;

/// A plain fixture plus the three addresses the matrix needs. They are
/// generated here rather than in `setup()` so that every other test keeps the
/// same generated addresses — and therefore the same snapshots.
struct AuthFixture {
    f: Fixture,
    /// Activated by the rows that need a live provider.
    provider: Address,
    /// Never registered: `register_provider` needs a fresh owner.
    new_provider: Address,
    /// Never used as admin: the nominee for the `accept_admin` row.
    new_admin: Address,
}

impl AuthFixture {
    fn new() -> Self {
        let f = setup();
        let provider = Address::generate(&f.env);
        let new_provider = Address::generate(&f.env);
        let new_admin = Address::generate(&f.env);
        Self {
            f,
            provider,
            new_provider,
            new_admin,
        }
    }
}

/// How the docs name a signer, and where the fixture keeps that address.
#[derive(Clone, Copy)]
struct Signer {
    label: &'static str,
    address: fn(&AuthFixture) -> Address,
}

/// The signers a function must demand.
#[derive(Clone, Copy)]
enum Auth {
    Signer(Signer),
    /// No signature at all; the note says why (a view, a one-shot setup, or a
    /// deliberately permissionless call).
    Free(&'static str),
}

impl Auth {
    fn label(&self) -> &'static str {
        match self {
            Auth::Signer(signer) => signer.label,
            Auth::Free(note) => note,
        }
    }
}

struct AuthCase {
    function: &'static str,
    auth: Auth,
}

const fn signer(label: &'static str, address: fn(&AuthFixture) -> Address) -> Auth {
    Auth::Signer(Signer { label, address })
}

fn admin_addr(f: &AuthFixture) -> Address {
    f.f.admin.clone()
}

fn provider_addr(f: &AuthFixture) -> Address {
    f.provider.clone()
}

fn new_provider_addr(f: &AuthFixture) -> Address {
    f.new_provider.clone()
}

fn new_admin_addr(f: &AuthFixture) -> Address {
    f.new_admin.clone()
}

/// Every public function of the registry, in declaration order.
const AUTH_MATRIX: &[AuthCase] = &[
    AuthCase {
        function: "initialize",
        auth: Auth::Free("none (one-shot)"),
    },
    AuthCase {
        function: "register_provider",
        auth: signer("`owner`", new_provider_addr),
    },
    AuthCase {
        function: "update_provider_category",
        auth: signer("`provider_addr`", provider_addr),
    },
    AuthCase {
        function: "set_provider_status",
        auth: signer("`admin`", admin_addr),
    },
    AuthCase {
        function: "set_verification_evidence",
        auth: signer("`admin`", admin_addr),
    },
    AuthCase {
        function: "retire",
        auth: signer("`owner`", provider_addr),
    },
    AuthCase {
        function: "upsert_service",
        auth: signer("`provider_addr`", provider_addr),
    },
    AuthCase {
        function: "remove_service",
        auth: signer("`provider_addr`", provider_addr),
    },
    AuthCase {
        function: "schedule_service_price",
        auth: signer("`provider_addr`", provider_addr),
    },
    AuthCase {
        function: "set_service_available",
        auth: signer("`provider_addr`", provider_addr),
    },
    AuthCase {
        function: "set_capacity",
        auth: signer("`provider_addr`", provider_addr),
    },
    AuthCase {
        function: "add_attester",
        auth: signer("`admin`", admin_addr),
    },
    AuthCase {
        function: "remove_attester",
        auth: signer("`admin`", admin_addr),
    },
    AuthCase {
        function: "propose_admin",
        auth: signer("`admin`", admin_addr),
    },
    AuthCase {
        function: "accept_admin",
        auth: signer("`new_admin`", new_admin_addr),
    },
    AuthCase {
        function: "providers_in",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "get_admin",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "get_pending_admin",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "get_provider",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "list_providers",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "provider_counts",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "is_active_provider",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "get_service",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "get_services",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "get_service_price",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "get_upcoming_price",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "is_service_available",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "is_attester",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "list_attesters",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "get_capacity",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "version",
        auth: Auth::Free("none (view)"),
    },
];

/// The signers the host recorded for the invocation that just ran.
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

/// Runs `function` in the state that makes it a valid call, so the only thing
/// under test is who it demands. Every arm leaves the function under test as
/// the last invocation, which is what `env.auths()` and the resource meter
/// report.
fn run_case(f: &AuthFixture, function: &str) -> CaseRun {
    let env = &f.f.env;
    let client = &f.f.client;
    let label = |text: &'static str| String::from_str(env, text);
    let upsert = || client.upsert_service(&f.provider, &CONSULT, &label("Consult"), &PRICE);

    match function {
        // One-shot setup needs a fresh contract, so it cannot share the
        // fixture: assert on an env that has never been initialised.
        "initialize" => {
            let env = Env::default();
            env.mock_all_auths();
            let address = env.register(Registry, ());
            RegistryClient::new(&env, &address).initialize(&Address::generate(&env));
            return case_run(&env);
        }
        "register_provider" => {
            client.register_provider(
                &f.new_provider,
                &label("Lagos Clinic"),
                &label("NG"),
                &ProviderCategory::Clinic,
            );
        }
        "update_provider_category" => {
            activate(&f.f, &f.provider);
            client.update_provider_category(&f.provider, &ProviderCategory::Pharmacy);
        }
        "set_provider_status" => {
            activate(&f.f, &f.provider);
            client.set_provider_status(
                &f.f.admin,
                &f.provider,
                &ProviderStatus::Suspended,
                &REASON_VERIFIED,
            );
        }
        "set_verification_evidence" => {
            activate(&f.f, &f.provider);
            let hash = BytesN::from_array(env, &[42u8; 32]);
            client.set_verification_evidence(&f.f.admin, &f.provider, &hash);
        }
        "retire" => {
            activate(&f.f, &f.provider);
            client.retire(&f.provider, &REASON_VERIFIED);
        }
        "upsert_service" => {
            activate(&f.f, &f.provider);
            upsert();
        }
        "remove_service" => {
            activate(&f.f, &f.provider);
            upsert();
            client.remove_service(&f.provider, &CONSULT);
        }
        "schedule_service_price" => {
            activate(&f.f, &f.provider);
            upsert();
            client.schedule_service_price(
                &f.provider,
                &CONSULT,
                &PRICE,
                &MIN_PRICE_NOTICE_SECS,
                &false,
            );
        }
        "get_upcoming_price" => {
            activate(&f.f, &f.provider);
            upsert();
            client.get_upcoming_price(&f.provider, &CONSULT);
        }
        "set_service_available" => {
            activate(&f.f, &f.provider);
            upsert();
            client.set_service_available(&f.provider, &CONSULT, &false);
        }
        "set_capacity" => {
            activate(&f.f, &f.provider);
            client.set_capacity(&f.provider, &5);
        }
        "get_capacity" => {
            client.get_capacity(&f.provider);
        }
        "add_attester" => {
            client.add_attester(&f.f.admin, &f.new_provider);
        }
        "remove_attester" => {
            client.add_attester(&f.f.admin, &f.new_provider);
            client.remove_attester(&f.f.admin, &f.new_provider);
        }
        "propose_admin" => {
            client.propose_admin(&f.f.admin, &f.new_admin);
        }
        "accept_admin" => {
            client.propose_admin(&f.f.admin, &f.new_admin);
            client.accept_admin(&f.new_admin);
        }
        "providers_in" => {
            activate(&f.f, &f.provider);
            client.providers_in(&String::from_str(env, "NG"), &0u32, &10u32);
        }
        "get_admin" => {
            client.get_admin();
        }
        "get_pending_admin" => {
            client.get_pending_admin();
        }
        "get_provider" => {
            activate(&f.f, &f.provider);
            client.get_provider(&f.provider);
        }
        "list_providers" => {
            client.list_providers(&0u32, &10u32);
        }
        "provider_counts" => {
            client.provider_counts();
        }
        "is_active_provider" => {
            activate(&f.f, &f.provider);
            client.is_active_provider(&f.provider);
        }
        "get_service" => {
            activate(&f.f, &f.provider);
            upsert();
            client.get_service(&f.provider, &CONSULT);
        }
        "get_services" => {
            activate(&f.f, &f.provider);
            upsert();
            client.get_services(&f.provider, &Vec::from_array(env, [CONSULT]));
        }
        "get_service_price" => {
            activate(&f.f, &f.provider);
            upsert();
            client.get_service_price(&f.provider, &CONSULT);
        }
        "is_service_available" => {
            activate(&f.f, &f.provider);
            upsert();
            client.is_service_available(&f.provider, &CONSULT);
        }
        "is_attester" => {
            client.is_attester(&f.provider);
        }
        "list_attesters" => {
            client.list_attesters(&0u32, &10u32);
        }
        "version" => {
            client.version();
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
                 a missing require_auth() lets anyone act for another party, and an \
                 extra one makes every caller collect a signature nobody needs",
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
             If that is intentional the function is no longer permissionless, and \
             every caller has to change",
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

/// The rows of the authorisation table in `docs/developers/contracts.md` for
/// one contract, as written.
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

    let rows = doc_rows(DOCS, "registry");
    assert_eq!(
        rows.len(),
        AUTH_MATRIX.len(),
        "docs/developers/contracts.md documents {} registry functions but the matrix has {}",
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
        "storage reads or writes changed for a registry entry point. If the change is \
         intentional, run `just storage-counts` and explain the new cost in the pull \
         request; if it is not, the call is doing more ledger work than it used to",
    );
}

// ----- event snapshots -----
//
// Events are an API for the indexer in `sci-healthcare-backend`, which decodes
// them by event name and field name. These tests read the decoded XDR event
// the contract publishes — the same bytes an indexer reads — and pin both the
// topic list and the data map for every function that emits one.
//
// What breaks if this is wrong: a renamed or moved field silently turns into
// `undefined` in the indexer, and every consumer of the read API loses data.

/// The single event published by the last invocation, from this contract.
fn one_event(f: &Fixture) -> ContractEvent {
    let events = f.env.events().all().filter_by_contract(&f.client.address);
    let list = events.events();
    assert_eq!(
        list.len(),
        1,
        "expected the invocation to publish exactly one registry event, got {:?}",
        list,
    );
    list[0].clone()
}

/// Topics of the event, as published (topic 0 is the event name).
fn topics(e: &ContractEvent) -> &[ScVal] {
    match &e.body {
        ContractEventBody::V0(v0) => &v0.topics,
    }
}

/// The event's data map, or `None` when it is not a map.
fn data(e: &ContractEvent) -> Option<&soroban_sdk::xdr::VecM<soroban_sdk::xdr::ScMapEntry>> {
    match &e.body {
        ContractEventBody::V0(v0) => match &v0.data {
            ScVal::Map(Some(map)) => Some(map),
            _ => None,
        },
    }
}

fn symbol(v: &ScVal) -> Option<&str> {
    match v {
        ScVal::Symbol(s) => (&s.0).try_into().ok(),
        _ => None,
    }
}

/// The data-map keys, in the order the map carries them (an XDR map is
/// sorted by key, so this is stable).
fn field_names(e: &ContractEvent) -> std::vec::Vec<&str> {
    data(e)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| symbol(&entry.key))
                .collect()
        })
        .unwrap_or_default()
}

/// One field of the data map, with a descriptive failure when it is missing.
fn field(e: &ContractEvent, name: &str) -> ScVal {
    let entries = data(e).unwrap_or_else(|| panic!("event data is not a map: {e:?}"));
    let found = entries
        .iter()
        .find(|entry| symbol(&entry.key) == Some(name))
        .unwrap_or_else(|| {
            let keys = field_names(e);
            panic!(
                "event has no `{name}` field; its fields are {keys:?}. \
                 The indexer in sci-healthcare-backend decodes by field name \
                 (TOPIC_FIELDS in sci-healthcare-backend/src/stellar/events.ts)",
            )
        });
    found.val.clone()
}

fn field_address(env: &Env, e: &ContractEvent, name: &str) -> Address {
    use soroban_sdk::TryFromVal;

    let value = field(e, name);
    match &value {
        ScVal::Address(_) => Address::try_from_val(env, &value)
            .unwrap_or_else(|_| panic!("event field `{name}` is not a usable address: {value:?}")),
        other => panic!("event field `{name}` is {other:?}, want an address"),
    }
}

fn field_u32(e: &ContractEvent, name: &str) -> u32 {
    match field(e, name) {
        ScVal::U32(v) => v,
        other => panic!("event field `{name}` is {other:?}, want u32"),
    }
}

fn field_u64(e: &ContractEvent, name: &str) -> u64 {
    match field(e, name) {
        ScVal::U64(v) => v,
        other => panic!("event field `{name}` is {other:?}, want u64"),
    }
}

fn field_i128(e: &ContractEvent, name: &str) -> i128 {
    match field(e, name) {
        ScVal::I128(parts) => i128::from(&parts),
        other => panic!("event field `{name}` is {other:?}, want i128"),
    }
}

fn field_text(e: &ContractEvent, name: &str) -> std::string::String {
    match field(e, name) {
        ScVal::String(s) => {
            let text: &str = (&s.0).try_into().expect("event string field is not UTF-8");
            text.into()
        }
        other => panic!("event field `{name}` is {other:?}, want a string"),
    }
}

/// Asserts the event name (topic 0), the extra topics, and the exact set of
/// data fields. Failing here means the layout an indexer reads has changed.
fn assert_layout(e: &ContractEvent, name: &str, extra_topics: &[ScVal], fields: &[&str]) {
    let published = topics(e);
    assert_eq!(
        published.len(),
        1 + extra_topics.len(),
        "event `{name}` publishes {} topics, expected {}. Topic 0 is the event \
         name and the rest are the #[topic] fields; moving a field between the \
         topic list and the data map is a breaking change for the indexer in \
         sci-healthcare-backend",
        published.len(),
        1 + extra_topics.len(),
    );
    assert_eq!(
        symbol(&published[0]),
        Some(name),
        "topic 0 names the event; renaming it breaks the decoder in \
         sci-healthcare-backend/src/stellar/events.ts",
    );
    for (index, want) in extra_topics.iter().enumerate() {
        assert_eq!(
            &published[index + 1],
            want,
            "topic {} of `{name}` changed; the indexer matches topics by position, \
             so this needs a matching change in sci-healthcare-backend/src/stellar/events.ts",
            index + 1,
        );
    }

    let mut want_fields: std::vec::Vec<&str> = fields.to_vec();
    want_fields.sort_unstable();
    assert_eq!(
        field_names(e),
        want_fields,
        "the data map of `{name}` changed. This is the layout the indexer decodes \
         (TOPIC_FIELDS in sci-healthcare-backend/src/stellar/events.ts); \
         update the decoder in the same change set or restore the field",
    );
}

#[test]
fn initialized_event_snapshot() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let address = env.register(Registry, ());
    RegistryClient::new(&env, &address).initialize(&admin);

    let events = env.events().all().filter_by_contract(&address);
    let list = events.events();
    assert_eq!(list.len(), 1, "initialize publishes one event");
    let e = list[0].clone();

    assert_layout(&e, "initialized", &[], &["admin"]);
    assert_eq!(field_address(&env, &e, "admin"), admin);
}

#[test]
fn provider_registered_event_snapshot() {
    let f = setup();
    f.env.ledger().with_mut(|li| li.timestamp = 1_700_000_000);
    let provider = Address::generate(&f.env);

    f.client.register_provider(
        &provider,
        &String::from_str(&f.env, "Lagos Clinic"),
        &String::from_str(&f.env, "NG"),
        &ProviderCategory::Clinic,
    );

    let e = one_event(&f);
    assert_layout(
        &e,
        "provider_registered",
        &[ScVal::from(&provider)],
        &["name", "country", "registered_at", "category"],
    );
    assert_eq!(field_text(&e, "name"), "Lagos Clinic");
    assert_eq!(field_text(&e, "country"), "NG");
    assert_eq!(field_u64(&e, "registered_at"), 1_700_000_000);
    assert_eq!(field_u32(&e, "category"), 0);
}

#[test]
fn provider_category_changed_event_snapshot() {
    let f = setup();
    let provider = register_active_provider(&f);

    f.client
        .update_provider_category(&provider, &ProviderCategory::Pharmacy);

    let e = one_event(&f);
    assert_layout(
        &e,
        "provider_category_changed",
        &[ScVal::from(&provider)],
        &["category"],
    );
    assert_eq!(field_u32(&e, "category"), 1);
}

#[test]
fn provider_status_changed_event_snapshot() {
    let f = setup();
    let provider = register_active_provider(&f);

    f.client.set_provider_status(
        &f.admin,
        &provider,
        &ProviderStatus::Suspended,
        &REASON_SUSPENDED_WHILE_TESTING,
    );

    let e = one_event(&f);
    assert_layout(
        &e,
        "provider_status_changed",
        &[ScVal::from(&provider)],
        &["reason_code", "status"],
    );
    // Unit enum with explicit discriminants: Suspended is 2.
    assert_eq!(field_u32(&e, "status"), 2);
    assert_eq!(field_u32(&e, "reason_code"), REASON_SUSPENDED_WHILE_TESTING);
}

#[test]
fn verification_evidence_set_event_snapshot() {
    let f = setup();
    let provider = register_active_provider(&f);
    let hash = BytesN::from_array(&f.env, &[7u8; 32]);

    f.client
        .set_verification_evidence(&f.admin, &provider, &hash);

    let e = one_event(&f);
    assert_layout(
        &e,
        "verification_evidence_set",
        &[ScVal::from(&provider)],
        &["evidence_hash"],
    );
}

#[test]
fn service_upserted_event_snapshot() {
    let f = setup();
    let provider = register_active_provider(&f);

    f.client.upsert_service(
        &provider,
        &CONSULT,
        &String::from_str(&f.env, "Outpatient consult"),
        &PRICE,
    );

    let e = one_event(&f);
    assert_layout(
        &e,
        "service_upserted",
        &[ScVal::from(&provider), ScVal::U32(CONSULT)],
        &["label", "price"],
    );
    assert_eq!(field_text(&e, "label"), "Outpatient consult");
    assert_eq!(field_i128(&e, "price"), PRICE);
}

#[test]
fn service_removed_event_snapshot() {
    let f = setup();
    let provider = register_active_provider(&f);
    f.client.upsert_service(
        &provider,
        &CONSULT,
        &String::from_str(&f.env, "Outpatient consult"),
        &PRICE,
    );

    f.client.remove_service(&provider, &CONSULT);

    let e = one_event(&f);
    assert_layout(&e, "service_removed", &[ScVal::from(&provider)], &["code"]);
    assert_eq!(field_u32(&e, "code"), CONSULT);
}

#[test]
fn service_price_scheduled_event_snapshot() {
    let f = setup();
    let provider = register_active_provider(&f);
    f.client.upsert_service(
        &provider,
        &CONSULT,
        &String::from_str(&f.env, "Outpatient consult"),
        &PRICE,
    );

    f.client.schedule_service_price(
        &provider,
        &CONSULT,
        &(PRICE * 2),
        &MIN_PRICE_NOTICE_SECS,
        &false,
    );

    let e = one_event(&f);
    assert_layout(
        &e,
        "service_price_scheduled",
        &[ScVal::from(&provider), ScVal::U32(CONSULT)],
        &["price", "effective_at"],
    );
    assert_eq!(field_i128(&e, "price"), PRICE * 2);
    assert_eq!(field_u64(&e, "effective_at"), MIN_PRICE_NOTICE_SECS);
}

#[test]
fn service_availability_changed_event_snapshot() {
    let f = setup();
    let provider = register_active_provider(&f);
    f.client.upsert_service(
        &provider,
        &CONSULT,
        &String::from_str(&f.env, "Outpatient consult"),
        &PRICE,
    );

    f.client.set_service_available(&provider, &CONSULT, &false);

    let e = one_event(&f);
    assert_layout(
        &e,
        "service_availability_changed",
        &[ScVal::from(&provider), ScVal::U32(CONSULT)],
        &["available"],
    );
    assert_eq!(field(&e, "available"), ScVal::Bool(false));
}

#[test]
fn attester_added_event_snapshot() {
    let f = setup();
    let attester = Address::generate(&f.env);

    f.client.add_attester(&f.admin, &attester);

    let e = one_event(&f);
    // Only topic fields: the data map carries nothing.
    assert_layout(&e, "attester_added", &[ScVal::from(&attester)], &[]);
}

#[test]
fn attester_removed_event_snapshot() {
    let f = setup();
    let attester = Address::generate(&f.env);
    f.client.add_attester(&f.admin, &attester);

    f.client.remove_attester(&f.admin, &attester);

    let e = one_event(&f);
    assert_layout(&e, "attester_removed", &[ScVal::from(&attester)], &[]);
}

#[test]
fn admin_proposed_event_snapshot() {
    let f = setup();
    let new_admin = Address::generate(&f.env);

    f.client.propose_admin(&f.admin, &new_admin);

    let e = one_event(&f);
    assert_layout(
        &e,
        "admin_proposed",
        &[ScVal::from(&f.admin)],
        &["pending_admin"],
    );
    assert_eq!(field_address(&f.env, &e, "pending_admin"), new_admin);
}

#[test]
fn admin_changed_event_snapshot() {
    let f = setup();
    let new_admin = Address::generate(&f.env);
    f.client.propose_admin(&f.admin, &new_admin);

    f.client.accept_admin(&new_admin);

    let e = one_event(&f);
    // `new_admin` is data, not a topic: an indexer reading it from the topic
    // list gets undefined, so the layout is pinned here.
    assert_layout(&e, "admin_changed", &[], &["new_admin"]);
    assert_eq!(field_address(&f.env, &e, "new_admin"), new_admin);
}

// ----- storage lifetime (TTL) -----
//
// Soroban archives persistent entries whose TTL lapses. The contracts extend
// TTL on every persistent read and write, and these tests jump the ledger far
// enough to prove it:
//
//   * entries touched by normal use are still readable, and get a fresh
//     90-day TTL once they drop below the 30-day threshold, and
//   * an entry nobody touches lapses at day 90 and is restored by the host on
//     the next read instead of being lost (see
//     docs/developers/contracts.md#storage-lifetime-ttl for what that costs).
//
// What breaks if this is wrong: a clinic whose registration lapses cannot be
// paid, and restoring the entry is a fee the user did not expect.

const DAY_IN_LEDGERS: u32 = 17_280;
const NINETY_DAYS: u32 = 90 * DAY_IN_LEDGERS;

fn advance_days(f: &Fixture, days: u32) {
    f.env.ledger().with_mut(|li| {
        li.sequence_number += days * DAY_IN_LEDGERS;
        li.timestamp += u64::from(days) * 24 * 60 * 60;
    });
}

fn provider_ttl(f: &Fixture, provider: &Address) -> u32 {
    let contract = f.client.address.clone();
    let key = DataKey::Provider(provider.clone());
    f.env
        .as_contract(&contract, || f.env.storage().persistent().get_ttl(&key))
}

// ----- clinic capacity -----

#[test]
fn capacity_changed_event_snapshot() {
    let f = setup();
    let provider = register_active_provider(&f);

    f.client.set_capacity(&provider, &12);

    let e = one_event(&f);
    assert_layout(
        &e,
        "capacity_changed",
        &[ScVal::from(&provider)],
        &["max_outstanding"],
    );
    assert_eq!(field_u32(&e, "max_outstanding"), 12);
}

// ----- country codes -----
//
// The app and the indexer group clinics by country code, so the registry
// accepts only what can be a country: two ASCII uppercase letters. What
// breaks if this is wrong: "ng" and "NG" showing up as two countries, or
// junk codes breaking the filter entirely.
//
// Checking against the full ISO list is an off-chain concern; the shape is
// not.

// ----- status transitions -----
//
// Only four transitions carry real information; every other pair of states
// — including same-to-same and anything touching `Retired` — must be
// refused with `InvalidTransition` instead of emitting an event that
// misleads the indexer. One test per pair of the 4x4 state machine.
//
// What breaks if this is wrong: an audit of the registry's history shows
// status changes that never happened, or a backwards `Active → Pending`
// nobody can explain.

/// A fresh provider left in `status`, ready for one transition attempt.
fn provider_in(f: &Fixture, status: ProviderStatus) -> Address {
    let provider = Address::generate(&f.env);
    f.client.register_provider(
        &provider,
        &String::from_str(&f.env, "Clinic"),
        &String::from_str(&f.env, "NG"),
        &ProviderCategory::Clinic,
    );
    match status {
        ProviderStatus::Pending => {}
        ProviderStatus::Retired => f.client.retire(&provider, &REASON_VERIFIED),
        other => f
            .client
            .set_provider_status(&f.admin, &provider, &other, &REASON_VERIFIED),
    }
    provider
}

/// The admin attempts `provider → to`, returning the typed error.
fn try_transition(f: &Fixture, provider: &Address, to: &ProviderStatus) -> RegistryError {
    f.client
        .try_set_provider_status(&f.admin, provider, to, &REASON_VERIFIED)
        .err()
        .unwrap()
        .unwrap()
}

fn assert_allowed(from: ProviderStatus, to: ProviderStatus) {
    let f = setup();
    let provider = provider_in(&f, from);
    f.client
        .set_provider_status(&f.admin, &provider, &to, &REASON_VERIFIED);
    assert_eq!(f.client.get_provider(&provider).status, to);
}

fn assert_rejected(from: ProviderStatus, to: ProviderStatus) {
    let f = setup();
    let provider = provider_in(&f, from);
    assert_eq!(
        try_transition(&f, &provider, &to),
        RegistryError::InvalidTransition,
        "`{from:?} → {to:?}` must be refused"
    );
    // The failed call changed nothing.
    assert_eq!(f.client.get_provider(&provider).status, from);
}

// ----- reason codes -----
//
// Every status change records why, on the provider and in the event, so a
// suspended clinic knows what to fix and a funder can tell fraud from
// paperwork. Codes are documented in `docs/protocol/trust.md` and are
// added, never renumbered.

/// `5` is "closed" — the reason a clinic gives when it retires.
const REASON_CLOSED: u32 = 5;

// ----- retiring -----
//
// A clinic that closes must be able to leave the registry itself, and
// staying left behind means funders keep sending money it will never claim.
// `Retired` is terminal: neither the clinic nor the admin can leave it.

// ----- event table in the docs -----
//
// Events are the indexer's API. This test reads the `#[contractevent]` structs
// in `events.rs` and compares the event names, topics and data fields with the
// table in `docs/developers/contracts.md`. Adding a field without documenting
// it fails here rather than in a downstream decoder.
//
// What breaks if this is wrong: `sci-healthcare-backend` decodes by event name
// and field name, so an undocumented rename becomes `undefined` in the indexer.

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
    const SOURCE: &str = include_str!("events.rs");

    let source = events_in_source(SOURCE);
    let rows = doc_event_rows(DOCS, "registry");
    assert_eq!(
        rows.len(),
        source.len(),
        "docs/developers/contracts.md documents {} registry events but the source has {}",
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
                     to the registry event table with topics {:?} and data {:?}",
                    event.name, event.topics, event.data,
                )
            });
        assert_eq!(
            split_doc_fields(&row[1]),
            event.topics,
            "the topics documented for `{}` do not match the #[topic] fields in \
             contracts/registry/src/events.rs",
            event.name,
        );
        assert_eq!(
            split_doc_fields(&row[2]),
            event.data,
            "the data fields documented for `{}` do not match the non-topic fields in \
             contracts/registry/src/events.rs. The indexer decodes them by name \
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
//
// What breaks if this is wrong: a deployed contract upgrade (or a redeploy
// pointed at existing state) loses access to providers and services already
// in the ledger.

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
    let env = Env::default();
    let owner = Address::from_str(
        &env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    );

    check_stored_type(
        &env,
        "provider",
        Provider {
            owner,
            name: String::from_str(&env, "Ikeja General Clinic"),
            country: String::from_str(&env, "NG"),
            status: ProviderStatus::Active,
            registered_at: 1_700_000_000,
            reason_code: REASON_VERIFIED,
            category: ProviderCategory::Clinic,
            verification_evidence: None,
        },
    );
    check_stored_type(
        &env,
        "service_item",
        ServiceItem {
            code: CONSULT,
            label: String::from_str(&env, "Outpatient consult"),
            price: PRICE,
            active: true,
            available: false,
            pending_price: PRICE + 5_000_000,
            effective_at: 1_700_086_400,
        },
    );
}

// ----- reinstating a suspended provider (#302) -----
//
// `Suspended → Active` re-opens a clinic to funding, so it is the most
// sensitive move in the state machine. Only the stored admin, signing, may
// make it, and it must always say why. Every other caller is tried below,
// and each failure is checked to leave the clinic suspended with its
// suspension reason intact.
//
// What breaks if this is wrong: a clinic suspended for fraud reinstates
// itself (or is reinstated by an attester or a stale admin key) and funders
// start escrowing money for it again, or it is reinstated with no recorded
// reason and nobody can tell from the history why it was trusted again.

const REASON_FRAUD: u32 = 4;

/// A registered, activated, then suspended (for fraud) provider.
fn suspended_provider(f: &Fixture) -> Address {
    let provider = register_active_provider(f);
    f.client.set_provider_status(
        &f.admin,
        &provider,
        &ProviderStatus::Suspended,
        &REASON_FRAUD,
    );
    provider
}

/// `caller` tries to reinstate `provider` with a valid reason; the typed error.
fn try_reinstate_as(f: &Fixture, caller: &Address, provider: &Address) -> RegistryError {
    f.client
        .try_set_provider_status(caller, provider, &ProviderStatus::Active, &REASON_VERIFIED)
        .err()
        .unwrap()
        .unwrap()
}

fn assert_still_suspended(f: &Fixture, provider: &Address) {
    let p = f.client.get_provider(provider);
    assert_eq!(p.status, ProviderStatus::Suspended);
    assert_eq!(
        p.reason_code, REASON_FRAUD,
        "the suspension reason must survive"
    );
    assert!(!f.client.is_active_provider(provider));
    assert_eq!(f.client.provider_counts(), (0, 0, 1));
}
