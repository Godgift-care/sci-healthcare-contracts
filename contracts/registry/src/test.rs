#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env, String, Vec,
};

use crate::{ProviderStatus, Registry, RegistryClient, RegistryError, MIN_PRICE_NOTICE_SECS};

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
    );
    f.client
        .set_provider_status(&f.admin, provider, &ProviderStatus::Active);
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

#[test]
fn provider_registers_as_pending() {
    let f = setup();
    let provider = Address::generate(&f.env);
    f.client.register_provider(
        &provider,
        &String::from_str(&f.env, "Nairobi Health Post"),
        &String::from_str(&f.env, "KE"),
    );

    let stored = f.client.get_provider(&provider);
    assert_eq!(stored.status, ProviderStatus::Pending);
    assert_eq!(stored.owner, provider);
    // Pending is not billable.
    assert!(!f.client.is_active_provider(&provider));
}

#[test]
fn duplicate_registration_rejected() {
    let f = setup();
    let provider = Address::generate(&f.env);
    let name = String::from_str(&f.env, "Clinic");
    let country = String::from_str(&f.env, "NG");
    f.client.register_provider(&provider, &name, &country);

    let err = f
        .client
        .try_register_provider(&provider, &name, &country)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::ProviderExists);
}

#[test]
fn bad_country_code_rejected() {
    let f = setup();
    let provider = Address::generate(&f.env);
    let err = f
        .client
        .try_register_provider(
            &provider,
            &String::from_str(&f.env, "Clinic"),
            &String::from_str(&f.env, "NGA"),
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::InvalidCountry);
}

#[test]
fn empty_name_rejected() {
    let f = setup();
    let provider = Address::generate(&f.env);
    let err = f
        .client
        .try_register_provider(
            &provider,
            &String::from_str(&f.env, ""),
            &String::from_str(&f.env, "NG"),
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::EmptyName);
}

#[test]
fn admin_activates_provider() {
    let f = setup();
    let provider = register_active_provider(&f);
    assert!(f.client.is_active_provider(&provider));
}

#[test]
fn non_admin_cannot_change_status() {
    let f = setup();
    let provider = register_active_provider(&f);
    let impostor = Address::generate(&f.env);

    let err = f
        .client
        .try_set_provider_status(&impostor, &provider, &ProviderStatus::Suspended)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::NotAuthorized);
}

#[test]
fn suspended_provider_is_not_active() {
    let f = setup();
    let provider = register_active_provider(&f);
    f.client
        .set_provider_status(&f.admin, &provider, &ProviderStatus::Suspended);
    assert!(!f.client.is_active_provider(&provider));
}

#[test]
fn unknown_provider_is_not_active() {
    let f = setup();
    let stranger = Address::generate(&f.env);
    assert!(!f.client.is_active_provider(&stranger));
}

#[test]
fn active_provider_manages_catalog() {
    let f = setup();
    let provider = register_active_provider(&f);

    f.client.upsert_service(
        &provider,
        &101u32,
        &String::from_str(&f.env, "Outpatient consult"),
        &30_000_000i128,
    );

    let item = f.client.get_service(&provider, &101u32);
    assert_eq!(item.price, 30_000_000i128);
    assert!(item.active);
}

#[test]
fn upsert_lowers_price_immediately() {
    let f = setup();
    let provider = register_active_provider(&f);
    let label = String::from_str(&f.env, "Malaria RDT");

    f.client
        .upsert_service(&provider, &202u32, &label, &12_500_000i128);
    f.client
        .upsert_service(&provider, &202u32, &label, &10_000_000i128);

    assert_eq!(
        f.client.get_service(&provider, &202u32).price,
        10_000_000i128
    );
}

#[test]
fn upsert_cannot_raise_price_without_notice() {
    let f = setup();
    let provider = register_active_provider(&f);
    let label = String::from_str(&f.env, "Malaria RDT");
    f.client
        .upsert_service(&provider, &202u32, &label, &10_000_000i128);

    let err = f
        .client
        .try_upsert_service(&provider, &202u32, &label, &12_500_000i128)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::PriceIncreaseNeedsNotice);
    assert_eq!(f.client.get_service_price(&provider, &202u32), 10_000_000);
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

#[test]
fn scheduled_price_applies_at_the_effective_time() {
    let f = setup();
    let provider = priced_service(&f);
    let effective_at = START + MIN_PRICE_NOTICE_SECS;

    f.client
        .schedule_service_price(&provider, &CONSULT, &(PRICE * 2), &effective_at);

    // Before: the current price stands and the upcoming one is visible.
    assert_eq!(f.client.get_service_price(&provider, &CONSULT), PRICE);
    assert_eq!(
        f.client.get_upcoming_price(&provider, &CONSULT),
        (PRICE * 2, effective_at)
    );
    f.env
        .ledger()
        .with_mut(|li| li.timestamp = effective_at - 1);
    assert_eq!(f.client.get_service_price(&provider, &CONSULT), PRICE);

    // At the effective time it applies, with nobody calling in.
    f.env.ledger().with_mut(|li| li.timestamp = effective_at);
    assert_eq!(f.client.get_service_price(&provider, &CONSULT), PRICE * 2);
    assert_eq!(f.client.get_service(&provider, &CONSULT).price, PRICE * 2);
    assert_eq!(f.client.get_upcoming_price(&provider, &CONSULT), (0, 0));
    let codes = Vec::from_array(&f.env, [CONSULT]);
    assert_eq!(
        f.client
            .get_services(&provider, &codes)
            .get_unchecked(0)
            .price,
        PRICE * 2
    );
}

#[test]
fn scheduled_price_needs_the_minimum_notice() {
    let f = setup();
    let provider = priced_service(&f);

    for effective_at in [START, START + MIN_PRICE_NOTICE_SECS - 1] {
        let err = f
            .client
            .try_schedule_service_price(&provider, &CONSULT, &(PRICE * 2), &effective_at)
            .err()
            .unwrap()
            .unwrap();
        assert_eq!(err, RegistryError::PriceNoticeTooShort);
    }
    assert_eq!(f.client.get_upcoming_price(&provider, &CONSULT), (0, 0));
}

#[test]
fn rescheduling_replaces_the_earlier_schedule() {
    let f = setup();
    let provider = priced_service(&f);
    let first = START + MIN_PRICE_NOTICE_SECS;
    let second = START + 2 * MIN_PRICE_NOTICE_SECS;

    f.client
        .schedule_service_price(&provider, &CONSULT, &(PRICE * 2), &first);
    // Scheduling the current price is how a clinic calls a change off.
    f.client
        .schedule_service_price(&provider, &CONSULT, &PRICE, &second);

    f.env.ledger().with_mut(|li| li.timestamp = first);
    assert_eq!(f.client.get_service_price(&provider, &CONSULT), PRICE);
    assert_eq!(
        f.client.get_upcoming_price(&provider, &CONSULT),
        (PRICE, second)
    );
}

#[test]
fn upsert_keeps_a_scheduled_price() {
    let f = setup();
    let provider = priced_service(&f);
    let effective_at = START + MIN_PRICE_NOTICE_SECS;
    f.client
        .schedule_service_price(&provider, &CONSULT, &(PRICE * 2), &effective_at);

    f.client.upsert_service(
        &provider,
        &CONSULT,
        &String::from_str(&f.env, "Consult (renamed)"),
        &PRICE,
    );

    assert_eq!(
        f.client.get_upcoming_price(&provider, &CONSULT),
        (PRICE * 2, effective_at)
    );
}

#[test]
fn schedule_rejects_bad_input() {
    let f = setup();
    let provider = priced_service(&f);
    let effective_at = START + MIN_PRICE_NOTICE_SECS;

    let err = f
        .client
        .try_schedule_service_price(&provider, &CONSULT, &0i128, &effective_at)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::InvalidPrice);

    let err = f
        .client
        .try_schedule_service_price(&provider, &999u32, &PRICE, &effective_at)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::ServiceNotFound);

    let err = f
        .client
        .try_get_upcoming_price(&provider, &999u32)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::ServiceNotFound);

    f.client
        .set_provider_status(&f.admin, &provider, &ProviderStatus::Suspended);
    let err = f
        .client
        .try_schedule_service_price(&provider, &CONSULT, &PRICE, &effective_at)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::ProviderNotActive);
}

#[test]
fn pending_provider_cannot_add_services() {
    let f = setup();
    let provider = Address::generate(&f.env);
    f.client.register_provider(
        &provider,
        &String::from_str(&f.env, "Unverified Clinic"),
        &String::from_str(&f.env, "NG"),
    );

    let err = f
        .client
        .try_upsert_service(
            &provider,
            &101u32,
            &String::from_str(&f.env, "Consult"),
            &30_000_000i128,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::ProviderNotActive);
}

#[test]
fn suspended_provider_cannot_reprice() {
    let f = setup();
    let provider = register_active_provider(&f);
    f.client.upsert_service(
        &provider,
        &101u32,
        &String::from_str(&f.env, "Consult"),
        &30_000_000i128,
    );
    f.client
        .set_provider_status(&f.admin, &provider, &ProviderStatus::Suspended);

    let err = f
        .client
        .try_upsert_service(
            &provider,
            &101u32,
            &String::from_str(&f.env, "Consult"),
            &1i128,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::ProviderNotActive);
}

#[test]
fn non_positive_price_rejected() {
    let f = setup();
    let provider = register_active_provider(&f);
    let label = String::from_str(&f.env, "Consult");

    for bad in [0i128, -1i128] {
        let err = f
            .client
            .try_upsert_service(&provider, &101u32, &label, &bad)
            .err()
            .unwrap()
            .unwrap();
        assert_eq!(err, RegistryError::InvalidPrice);
    }
}

#[test]
fn removing_absent_service_errors() {
    let f = setup();
    let provider = register_active_provider(&f);
    let err = f
        .client
        .try_remove_service(&provider, &999u32)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::ServiceNotFound);
}

#[test]
fn remove_service_clears_it() {
    let f = setup();
    let provider = register_active_provider(&f);
    f.client.upsert_service(
        &provider,
        &101u32,
        &String::from_str(&f.env, "Consult"),
        &30_000_000i128,
    );
    f.client.remove_service(&provider, &101u32);

    let err = f
        .client
        .try_get_service(&provider, &101u32)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::ServiceNotFound);
}

#[test]
fn get_services_batches_and_skips_missing() {
    let f = setup();
    let provider = register_active_provider(&f);
    f.client.upsert_service(
        &provider,
        &101u32,
        &String::from_str(&f.env, "Consult"),
        &30_000_000i128,
    );
    f.client.upsert_service(
        &provider,
        &202u32,
        &String::from_str(&f.env, "Malaria RDT"),
        &10_000_000i128,
    );

    let codes = Vec::from_array(&f.env, [101u32, 202u32, 999u32]);
    let items = f.client.get_services(&provider, &codes);
    assert_eq!(items.len(), 2);
}

#[test]
fn new_service_is_available() {
    let f = setup();
    let provider = register_active_provider(&f);
    f.client.upsert_service(
        &provider,
        &202u32,
        &String::from_str(&f.env, "Malaria RDT"),
        &10_000_000i128,
    );

    assert!(f.client.get_service(&provider, &202u32).available);
    assert!(f.client.is_service_available(&provider, &202u32));
}

#[test]
fn paused_service_stays_listed_and_priced() {
    let f = setup();
    let provider = register_active_provider(&f);
    f.client.upsert_service(
        &provider,
        &202u32,
        &String::from_str(&f.env, "Malaria RDT"),
        &10_000_000i128,
    );

    f.client.set_service_available(&provider, &202u32, &false);

    let item = f.client.get_service(&provider, &202u32);
    assert!(!item.available);
    assert!(item.active);
    assert!(!f.client.is_service_available(&provider, &202u32));
    // Still priced, so `voucher` can say "unavailable" rather than "not offered".
    assert_eq!(
        f.client.get_service_price(&provider, &202u32),
        10_000_000i128
    );
    let codes = Vec::from_array(&f.env, [202u32]);
    assert_eq!(f.client.get_services(&provider, &codes).len(), 1);

    f.client.set_service_available(&provider, &202u32, &true);
    assert!(f.client.is_service_available(&provider, &202u32));
}

#[test]
fn repricing_a_paused_service_keeps_it_paused() {
    let f = setup();
    let provider = register_active_provider(&f);
    let label = String::from_str(&f.env, "Malaria RDT");
    f.client
        .upsert_service(&provider, &202u32, &label, &10_000_000i128);
    f.client.set_service_available(&provider, &202u32, &false);

    f.client
        .upsert_service(&provider, &202u32, &label, &9_000_000i128);

    assert!(!f.client.get_service(&provider, &202u32).available);
}

#[test]
fn pausing_absent_service_errors() {
    let f = setup();
    let provider = register_active_provider(&f);
    let err = f
        .client
        .try_set_service_available(&provider, &999u32, &false)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::ServiceNotFound);
    assert!(!f.client.is_service_available(&provider, &999u32));
}

#[test]
fn attester_lifecycle() {
    let f = setup();
    let attester = Address::generate(&f.env);
    assert!(!f.client.is_attester(&attester));

    f.client.add_attester(&f.admin, &attester);
    assert!(f.client.is_attester(&attester));

    f.client.remove_attester(&f.admin, &attester);
    assert!(!f.client.is_attester(&attester));
}

#[test]
fn non_admin_cannot_add_attester() {
    let f = setup();
    let impostor = Address::generate(&f.env);
    let attester = Address::generate(&f.env);

    let err = f
        .client
        .try_add_attester(&impostor, &attester)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::NotAuthorized);
}

#[test]
fn admin_can_be_transferred() {
    let f = setup();
    let new_admin = Address::generate(&f.env);
    f.client.propose_admin(&f.admin, &new_admin);
    // Nothing changes until the nominee accepts.
    assert_eq!(f.client.get_admin(), f.admin);
    assert_eq!(f.client.get_pending_admin(), Some(new_admin.clone()));

    f.client.accept_admin(&new_admin);
    assert_eq!(f.client.get_admin(), new_admin);
    assert_eq!(f.client.get_pending_admin(), None);

    // Old admin loses authority.
    let attester = Address::generate(&f.env);
    let err = f
        .client
        .try_add_attester(&f.admin, &attester)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::NotAuthorized);
}

#[test]
fn service_price_view_returns_price() {
    let f = setup();
    let provider = register_active_provider(&f);
    f.client.upsert_service(
        &provider,
        &101u32,
        &String::from_str(&f.env, "Consult"),
        &30_000_000i128,
    );
    assert_eq!(
        f.client.get_service_price(&provider, &101u32),
        30_000_000i128
    );
}

#[test]
fn service_price_view_returns_zero_when_absent() {
    let f = setup();
    let provider = register_active_provider(&f);
    assert_eq!(f.client.get_service_price(&provider, &999u32), 0);

    let stranger = Address::generate(&f.env);
    assert_eq!(f.client.get_service_price(&stranger, &101u32), 0);
}

#[test]
fn only_the_nominee_can_accept_admin() {
    let f = setup();
    let nominee = Address::generate(&f.env);
    let intruder = Address::generate(&f.env);
    f.client.propose_admin(&f.admin, &nominee);

    let err = f.client.try_accept_admin(&intruder).err().unwrap().unwrap();
    assert_eq!(err, RegistryError::NotAuthorized);
    assert_eq!(f.client.get_admin(), f.admin);
}

#[test]
fn accept_admin_without_proposal_errors() {
    let f = setup();
    let someone = Address::generate(&f.env);
    let err = f.client.try_accept_admin(&someone).err().unwrap().unwrap();
    assert_eq!(err, RegistryError::NoPendingAdmin);
}

#[test]
fn non_admin_cannot_propose_admin() {
    let f = setup();
    let someone = Address::generate(&f.env);
    let err = f
        .client
        .try_propose_admin(&someone, &someone)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::NotAuthorized);
}

#[test]
fn reproposing_replaces_the_nominee() {
    let f = setup();
    let typo = Address::generate(&f.env);
    let intended = Address::generate(&f.env);
    f.client.propose_admin(&f.admin, &typo);
    f.client.propose_admin(&f.admin, &intended);

    let err = f.client.try_accept_admin(&typo).err().unwrap().unwrap();
    assert_eq!(err, RegistryError::NotAuthorized);
    f.client.accept_admin(&intended);
    assert_eq!(f.client.get_admin(), intended);
}

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
        function: "set_provider_status",
        auth: signer("`admin`", admin_addr),
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

/// Runs `function` in the state that makes it a valid call, so the only thing
/// under test is who it demands. Every arm leaves the function under test as
/// the last invocation, which is what `env.auths()` reports.
fn run_case(f: &AuthFixture, function: &str) -> std::vec::Vec<Address> {
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
            return recorded_auths(&env);
        }
        "register_provider" => {
            client.register_provider(&f.new_provider, &label("Lagos Clinic"), &label("NG"));
        }
        "set_provider_status" => {
            activate(&f.f, &f.provider);
            client.set_provider_status(&f.f.admin, &f.provider, &ProviderStatus::Suspended);
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
            client.schedule_service_price(&f.provider, &CONSULT, &PRICE, &MIN_PRICE_NOTICE_SECS);
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
        "version" => {
            client.version();
        }
        other => {
            panic!("no call is defined for `{other}`, so the matrix test cannot check it")
        }
    }

    recorded_auths(env)
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
        let got = run_case(&f, case.function);
        assert_demands(&f, case, &got);
    }
}

/// The rows of the authorisation table in `docs/developers/contracts.md` for
/// one contract, as written.
fn doc_rows<'a>(docs: &'a str, contract: &str) -> std::vec::Vec<&'a str> {
    let heading = std::format!("## {contract}\n");
    let start = docs
        .find(&heading)
        .unwrap_or_else(|| panic!("contracts.md has no `{contract}` section"));
    let section = &docs[start..];
    let start = section
        .find("### Authorisation matrix")
        .unwrap_or_else(|| panic!("the `{contract}` section has no authorisation matrix"));
    let section = &section[start..];
    // The matrix is its own subsection: stop at the next heading so the
    // errors and events tables further down are not counted as rows.
    let end = section
        .find("\n### ")
        .or_else(|| section.find("\n## "))
        .unwrap_or(section.len());
    section[..end]
        .lines()
        .filter(|line| line.starts_with("| `"))
        .collect()
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
    );

    let e = one_event(&f);
    assert_layout(
        &e,
        "provider_registered",
        &[ScVal::from(&provider)],
        &["name", "country", "registered_at"],
    );
    assert_eq!(field_text(&e, "name"), "Lagos Clinic");
    assert_eq!(field_text(&e, "country"), "NG");
    assert_eq!(field_u64(&e, "registered_at"), 1_700_000_000);
}

#[test]
fn provider_status_changed_event_snapshot() {
    let f = setup();
    let provider = register_active_provider(&f);

    f.client
        .set_provider_status(&f.admin, &provider, &ProviderStatus::Suspended);

    let e = one_event(&f);
    assert_layout(
        &e,
        "provider_status_changed",
        &[ScVal::from(&provider)],
        &["status"],
    );
    // Unit enum with explicit discriminants: Suspended is 2.
    assert_eq!(field_u32(&e, "status"), 2);
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

    f.client
        .schedule_service_price(&provider, &CONSULT, &(PRICE * 2), &MIN_PRICE_NOTICE_SECS);

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

#[test]
fn provider_service_and_attester_survive_sixty_days_of_activity() {
    let f = setup();
    let provider = register_active_provider(&f);
    f.client.upsert_service(
        &provider,
        &CONSULT,
        &String::from_str(&f.env, "Outpatient consult"),
        &PRICE,
    );
    let attester = Address::generate(&f.env);
    f.client.add_attester(&f.admin, &attester);

    // Ten days at a time, the way an indexer and the app poll the registry.
    for day in 1..=6 {
        advance_days(&f, 10);
        assert_eq!(
            f.client.get_provider(&provider).status,
            ProviderStatus::Active,
            "provider was archived on day {}",
            day * 10,
        );
        assert_eq!(f.client.get_service(&provider, &CONSULT).price, PRICE);
        assert!(f.client.is_attester(&attester));

        if day == 3 {
            // A suspension and reinstatement mid-way: normal admin writes
            // extend the whole contract instance, not just one entry.
            f.client
                .set_provider_status(&f.admin, &provider, &ProviderStatus::Suspended);
            f.client
                .set_provider_status(&f.admin, &provider, &ProviderStatus::Active);
        }
    }

    // Day 60. The registration written on day 0 had a 90-day TTL, so the
    // read above still found it live, and by now it has crossed the 30-day
    // threshold and been pushed back out to a full 90 days.
    assert_eq!(provider_ttl(&f, &provider), NINETY_DAYS);
}

#[test]
fn registry_reads_restore_a_lapsed_entry() {
    let f = setup();
    let provider = register_active_provider(&f);

    // Nothing touches the provider for 95 days: past the 90-day TTL the
    // contract set on it.
    advance_days(&f, 95);

    // The value is not lost. The host restores the lapsed entry on the first
    // read, and this contract's read path immediately extends it again.
    assert_eq!(f.client.get_provider(&provider).owner, provider);
    assert_eq!(provider_ttl(&f, &provider), NINETY_DAYS);

    // And it is still billable: a voucher for this provider can be funded.
    assert!(f.client.is_active_provider(&provider));
}

#[test]
fn not_initialized_errors() {
    let env = Env::default();
    env.mock_all_auths();
    let a = Address::generate(&env);
    let addr = env.register(Registry, ());
    let client = RegistryClient::new(&env, &addr);

    // Calls that read the admin before initialize should return NotInitialized.
    let err = client.try_get_admin().err().unwrap().unwrap();
    assert_eq!(err, RegistryError::NotInitialized);
    // get_provider returns ProviderNotFound because the provider doesn't exist,
    // not because the contract isn't initialized.
    let err = client.try_get_provider(&a).err().unwrap().unwrap();
    assert_eq!(err, RegistryError::ProviderNotFound);
}

#[test]
fn provider_not_found() {
    let f = setup();
    let unknown = Address::generate(&f.env);
    let err = f.client.try_get_provider(&unknown).err().unwrap().unwrap();
    assert_eq!(err, RegistryError::ProviderNotFound);
    // upsert_service returns ProviderNotFound when the provider doesn't exist,
    // because it calls read_provider first.
    let err = f
        .client
        .try_upsert_service(&unknown, &1u32, &String::from_str(&f.env, "X"), &100i128)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::ProviderNotFound);
    // set_service_available calls read_service directly (not read_provider),
    // so it returns ServiceNotFound.
    let err = f
        .client
        .try_set_service_available(&unknown, &1u32, &true)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, RegistryError::ServiceNotFound);
}
