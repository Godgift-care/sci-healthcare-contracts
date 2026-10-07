#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

use crate::{ProviderStatus, Registry, RegistryClient, RegistryError};

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

fn register_active_provider(f: &Fixture) -> Address {
    let provider = Address::generate(&f.env);
    f.client.register_provider(
        &provider,
        &String::from_str(&f.env, "Ikeja General Clinic"),
        &String::from_str(&f.env, "NG"),
    );
    f.client
        .set_provider_status(&f.admin, &provider, &ProviderStatus::Active);
    provider
}

#[test]
fn initialize_sets_admin() {
    let f = setup();
    assert_eq!(f.client.get_admin(), f.admin);
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
fn upsert_overwrites_price() {
    let f = setup();
    let provider = register_active_provider(&f);
    let label = String::from_str(&f.env, "Malaria RDT");

    f.client
        .upsert_service(&provider, &202u32, &label, &10_000_000i128);
    f.client
        .upsert_service(&provider, &202u32, &label, &12_500_000i128);

    assert_eq!(
        f.client.get_service(&provider, &202u32).price,
        12_500_000i128
    );
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
