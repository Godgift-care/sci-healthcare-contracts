#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, BytesN, Env};

use crate::{ReceiptBook, ReceiptBookClient, ReceiptError};

struct Fixture {
    env: Env,
    client: ReceiptBookClient<'static>,
    admin: Address,
    minter: Address,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(ReceiptBook, ());
    let client = ReceiptBookClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let minter = Address::generate(&env);
    client.initialize(&admin, &minter);

    Fixture {
        env,
        client,
        admin,
        minter,
    }
}

fn bref(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

#[test]
fn initialize_sets_admin_and_minter() {
    let f = setup();
    assert_eq!(f.client.get_admin(), f.admin);
    assert_eq!(f.client.get_minter(), f.minter);
}

#[test]
fn initialize_is_single_shot() {
    let f = setup();
    let other = Address::generate(&f.env);
    let err = f
        .client
        .try_initialize(&other, &other)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, ReceiptError::AlreadyInitialized);
}

#[test]
fn minter_mints_receipt() {
    let f = setup();
    let provider = Address::generate(&f.env);
    let patient = bref(&f.env, 7);

    f.client.mint(
        &f.minter,
        &1u64,
        &patient,
        &provider,
        &101u32,
        &30_000_000i128,
    );

    let r = f.client.get_receipt(&1u64);
    assert_eq!(r.voucher_id, 1);
    assert_eq!(r.provider, provider);
    assert_eq!(r.service_code, 101);
    assert_eq!(r.amount, 30_000_000i128);
    assert_eq!(r.beneficiary_ref, patient);
}

#[test]
fn non_minter_cannot_mint() {
    let f = setup();
    let impostor = Address::generate(&f.env);
    let provider = Address::generate(&f.env);

    let err = f
        .client
        .try_mint(
            &impostor,
            &1u64,
            &bref(&f.env, 7),
            &provider,
            &101u32,
            &30_000_000i128,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, ReceiptError::NotAuthorized);
}

#[test]
fn receipts_are_not_mintable_twice() {
    let f = setup();
    let provider = Address::generate(&f.env);
    let patient = bref(&f.env, 7);

    f.client.mint(
        &f.minter,
        &1u64,
        &patient,
        &provider,
        &101u32,
        &30_000_000i128,
    );

    let err = f
        .client
        .try_mint(
            &f.minter,
            &1u64,
            &patient,
            &provider,
            &101u32,
            &30_000_000i128,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, ReceiptError::ReceiptExists);
}

#[test]
fn non_positive_amount_rejected() {
    let f = setup();
    let provider = Address::generate(&f.env);

    let err = f
        .client
        .try_mint(
            &f.minter,
            &1u64,
            &bref(&f.env, 7),
            &provider,
            &101u32,
            &0i128,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, ReceiptError::InvalidAmount);
}

#[test]
fn count_tracks_episodes_per_beneficiary() {
    let f = setup();
    let provider = Address::generate(&f.env);
    let alice = bref(&f.env, 1);
    let bob = bref(&f.env, 2);

    assert_eq!(f.client.count_for(&alice), 0);

    f.client
        .mint(&f.minter, &1u64, &alice, &provider, &101u32, &10i128);
    f.client
        .mint(&f.minter, &2u64, &alice, &provider, &202u32, &20i128);
    f.client
        .mint(&f.minter, &3u64, &bob, &provider, &101u32, &30i128);

    assert_eq!(f.client.count_for(&alice), 2);
    assert_eq!(f.client.count_for(&bob), 1);
}

#[test]
fn missing_receipt_errors() {
    let f = setup();
    let err = f.client.try_get_receipt(&404u64).err().unwrap().unwrap();
    assert_eq!(err, ReceiptError::ReceiptNotFound);
}

#[test]
fn admin_can_repoint_minter() {
    let f = setup();
    let new_minter = Address::generate(&f.env);
    f.client.set_minter(&f.admin, &new_minter);
    assert_eq!(f.client.get_minter(), new_minter);

    // Old minter loses the right to mint.
    let provider = Address::generate(&f.env);
    let err = f
        .client
        .try_mint(
            &f.minter,
            &1u64,
            &bref(&f.env, 7),
            &provider,
            &101u32,
            &10i128,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, ReceiptError::NotAuthorized);
}

#[test]
fn non_admin_cannot_repoint_minter() {
    let f = setup();
    let impostor = Address::generate(&f.env);
    let err = f
        .client
        .try_set_minter(&impostor, &impostor)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, ReceiptError::NotAuthorized);
}

#[test]
fn admin_handover_takes_two_steps() {
    let f = setup();
    let new_admin = Address::generate(&f.env);
    f.client.propose_admin(&f.admin, &new_admin);
    assert_eq!(f.client.get_admin(), f.admin);

    f.client.accept_admin(&new_admin);
    assert_eq!(f.client.get_admin(), new_admin);
    assert_eq!(f.client.get_pending_admin(), None);

    // The old admin can no longer repoint the minter.
    let err = f
        .client
        .try_set_minter(&f.admin, &new_admin)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, ReceiptError::NotAuthorized);
}

#[test]
fn only_the_nominee_can_accept_admin() {
    let f = setup();
    let nominee = Address::generate(&f.env);
    let err = f.client.try_accept_admin(&nominee).err().unwrap().unwrap();
    assert_eq!(err, ReceiptError::NoPendingAdmin);

    f.client.propose_admin(&f.admin, &nominee);
    let intruder = Address::generate(&f.env);
    let err = f.client.try_accept_admin(&intruder).err().unwrap().unwrap();
    assert_eq!(err, ReceiptError::NotAuthorized);
}
