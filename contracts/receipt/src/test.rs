#![cfg(test)]

use std::string::ToString;

use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String};

use crate::{
    ReceiptBook, ReceiptBookClient, ReceiptError, MAX_RECEIPTS_PAGE, MAX_RECEIPTS_PER_CALL,
};

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
        &0i128,
        &30_000_000i128,
    );

    let r = f.client.get_receipt(&1u64);
    assert_eq!(r.voucher_id, 1);
    assert_eq!(r.provider, provider);
    assert_eq!(r.service_code, 101);
    assert_eq!(r.amount, 30_000_000i128);
    assert_eq!(r.fee, 0);
    assert_eq!(r.net, 30_000_000i128);
    assert_eq!(r.beneficiary_ref, patient);
}

#[test]
fn fee_and_net_are_recorded_on_the_receipt() {
    let f = setup();
    let provider = Address::generate(&f.env);
    let patient = bref(&f.env, 7);

    // A 1% protocol fee on a 30 USDC voucher, as the voucher contract pays it.
    f.client.mint(
        &f.minter,
        &1u64,
        &patient,
        &provider,
        &101u32,
        &30_000_000i128,
        &300_000i128,
        &29_700_000i128,
    );

    let r = f.client.get_receipt(&1u64);
    assert_eq!(r.amount, 30_000_000);
    assert_eq!(r.fee, 300_000);
    assert_eq!(r.net, 29_700_000);
    assert_eq!(r.net + r.fee, r.amount);
}

#[test]
fn fee_and_net_must_add_up_to_the_amount() {
    let f = setup();
    let provider = Address::generate(&f.env);
    let patient = bref(&f.env, 7);

    // A split that does not add up would misstate what the clinic received.
    let err = f
        .client
        .try_mint(
            &f.minter,
            &1u64,
            &patient,
            &provider,
            &101u32,
            &30_000_000i128,
            &300_000i128,
            &29_000_000i128,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, ReceiptError::AmountSplitMismatch);
    assert!(f.client.try_get_receipt(&1u64).is_err());

    // A negative side is not an amount at all.
    let err = f
        .client
        .try_mint(
            &f.minter,
            &1u64,
            &patient,
            &provider,
            &101u32,
            &30_000_000i128,
            &-1i128,
            &30_000_001i128,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, ReceiptError::InvalidAmount);
    assert!(f.client.try_get_receipt(&1u64).is_err());
}

#[test]
fn get_receipts_keeps_the_input_order_and_skips_unknown_ids() {
    let f = setup();
    let provider = Address::generate(&f.env);
    let patient = bref(&f.env, 7);

    for id in 1..=3u64 {
        f.client.mint(
            &f.minter,
            &id,
            &patient,
            &provider,
            &101u32,
            &(id as i128 * 10),
            &0i128,
            &(id as i128 * 10),
        );
    }

    // 9 does not exist; the known ids keep the order they were asked for.
    let ids = soroban_sdk::Vec::from_array(&f.env, [3u64, 9u64, 1u64]);
    let receipts = f.client.get_receipts(&ids);
    assert_eq!(receipts.len(), 2);
    assert_eq!(receipts.get(0).unwrap().voucher_id, 3);
    assert_eq!(receipts.get(0).unwrap().net, 30);
    assert_eq!(receipts.get(1).unwrap().voucher_id, 1);

    // Nothing asked for is an empty list, not an error.
    let none = f.client.get_receipts(&soroban_sdk::Vec::new(&f.env));
    assert_eq!(none.len(), 0);
}

#[test]
fn get_receipts_caps_the_batch() {
    let f = setup();
    let provider = Address::generate(&f.env);
    let patient = bref(&f.env, 7);

    let total = MAX_RECEIPTS_PER_CALL + 5;
    let mut ids = soroban_sdk::Vec::new(&f.env);
    for id in 1..=total as u64 {
        f.client.mint(
            &f.minter, &id, &patient, &provider, &101u32, &10i128, &0i128, &10i128,
        );
        ids.push_back(id);
    }

    // The cap is enforced: ids past it are ignored, exactly like unknown ids.
    let receipts = f.client.get_receipts(&ids);
    assert_eq!(receipts.len(), MAX_RECEIPTS_PER_CALL);
    assert_eq!(receipts.get(0).unwrap().voucher_id, 1);
    assert_eq!(
        receipts.get(MAX_RECEIPTS_PER_CALL - 1).unwrap().voucher_id,
        MAX_RECEIPTS_PER_CALL as u64,
    );
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
            &0i128,
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
        &0i128,
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
            &0i128,
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
            &0i128,
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

    f.client.mint(
        &f.minter, &1u64, &alice, &provider, &101u32, &10i128, &0i128, &10i128,
    );
    f.client.mint(
        &f.minter, &2u64, &alice, &provider, &202u32, &20i128, &0i128, &20i128,
    );
    f.client.mint(
        &f.minter, &3u64, &bob, &provider, &101u32, &30i128, &0i128, &30i128,
    );

    assert_eq!(f.client.count_for(&alice), 2);
    assert_eq!(f.client.count_for(&bob), 1);
}

#[test]
fn provider_stats_report_exact_count_and_total() {
    let f = setup();
    let clinic_a = Address::generate(&f.env);
    let clinic_b = Address::generate(&f.env);
    let patient = bref(&f.env, 7);

    // A clinic nobody has minted for yet reports zeros, not an error.
    assert_eq!(f.client.provider_stats(&clinic_a), (0, 0));

    // Interleave the two clinics so a shared or leaky counter would show.
    f.client.mint(
        &f.minter,
        &1u64,
        &patient,
        &clinic_a,
        &101u32,
        &30_000_000i128,
        &0i128,
        &30_000_000i128,
    );
    f.client.mint(
        &f.minter,
        &2u64,
        &patient,
        &clinic_b,
        &101u32,
        &10_000_000i128,
        &0i128,
        &10_000_000i128,
    );
    f.client.mint(
        &f.minter,
        &3u64,
        &patient,
        &clinic_a,
        &202u32,
        &40_000_000i128,
        &0i128,
        &40_000_000i128,
    );
    f.client.mint(
        &f.minter, &4u64, &patient, &clinic_a, &101u32, &5i128, &0i128, &5i128,
    );

    assert_eq!(f.client.provider_stats(&clinic_a), (3, 70_000_005));
    assert_eq!(f.client.provider_stats(&clinic_b), (1, 10_000_000));

    // The count agrees with the per-beneficiary and per-clinic views: one
    // set of receipts, three ways of reading it.
    assert_eq!(f.client.count_for(&patient), 4);
    assert_eq!(
        f.client
            .receipts_of_provider(&clinic_a, &0u32, &50u32)
            .len(),
        3
    );
}

#[test]
fn provider_count_overflow_is_refused() {
    // The count is the clinic's index length as well as its track record.
    // One more receipt would wrap it, so the mint must be refused rather
    // than report a clinic with u32::MAX receipts and a fresh index.
    let f = setup();
    let provider = Address::generate(&f.env);
    let patient = bref(&f.env, 7);
    f.client.mint(
        &f.minter, &1u64, &patient, &provider, &101u32, &10i128, &0i128, &10i128,
    );

    let contract = f.client.address.clone();
    f.env.as_contract(&contract, || {
        f.env
            .storage()
            .persistent()
            .set(&DataKey::ProviderCount(provider.clone()), &u32::MAX);
    });

    let err = f
        .client
        .try_mint(
            &f.minter, &2u64, &patient, &provider, &101u32, &10i128, &0i128, &10i128,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, ReceiptError::MathOverflow);

    // The refused mint wrote nothing: no receipt, no wrapped counter.
    assert!(f.client.try_get_receipt(&2u64).is_err());
    assert_eq!(f.client.provider_stats(&provider), (u32::MAX, 10i128));
}

#[test]
fn provider_total_overflow_is_refused() {
    // The total is the money a clinic has been paid. Wrapping it would
    // understate the record everyone is meant to check, so the mint fails.
    let f = setup();
    let provider = Address::generate(&f.env);
    let patient = bref(&f.env, 7);
    f.client.mint(
        &f.minter, &1u64, &patient, &provider, &101u32, &10i128, &0i128, &10i128,
    );

    let contract = f.client.address.clone();
    let near_max = i128::MAX - 5;
    f.env.as_contract(&contract, || {
        f.env
            .storage()
            .persistent()
            .set(&DataKey::ProviderTotal(provider.clone()), &near_max);
    });

    let err = f
        .client
        .try_mint(
            &f.minter, &2u64, &patient, &provider, &101u32, &10i128, &0i128, &10i128,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, ReceiptError::MathOverflow);

    // No receipt, and the running total still says what it said before.
    assert!(f.client.try_get_receipt(&2u64).is_err());
    assert_eq!(f.client.provider_stats(&provider), (1, near_max));
}

#[test]
fn provider_receipts_are_in_mint_order_and_never_mix() {
    let f = setup();
    let clinic_a = Address::generate(&f.env);
    let clinic_b = Address::generate(&f.env);
    let patient = bref(&f.env, 7);

    // Interleave the two clinics' receipts: the index must keep each
    // clinic's own order, not the global mint order.
    f.client.mint(
        &f.minter, &1u64, &patient, &clinic_a, &101u32, &10i128, &0i128, &10i128,
    );
    f.client.mint(
        &f.minter, &2u64, &patient, &clinic_b, &101u32, &20i128, &0i128, &20i128,
    );
    f.client.mint(
        &f.minter, &3u64, &patient, &clinic_a, &202u32, &30i128, &0i128, &30i128,
    );
    f.client.mint(
        &f.minter, &4u64, &patient, &clinic_b, &202u32, &40i128, &0i128, &40i128,
    );

    let a = f.client.receipts_of_provider(&clinic_a, &0u32, &10u32);
    assert_eq!(a.len(), 2);
    assert_eq!(a.get(0).unwrap().voucher_id, 1);
    assert_eq!(a.get(1).unwrap().voucher_id, 3);

    let b = f.client.receipts_of_provider(&clinic_b, &0u32, &10u32);
    assert_eq!(b.len(), 2);
    assert_eq!(b.get(0).unwrap().voucher_id, 2);
    assert_eq!(b.get(1).unwrap().voucher_id, 4);

    // A clinic with no receipts gets an empty list, not another's history.
    let clinic_c = Address::generate(&f.env);
    assert_eq!(
        f.client
            .receipts_of_provider(&clinic_c, &0u32, &10u32)
            .len(),
        0
    );
}

#[test]
fn provider_receipts_page_and_cap_the_limit() {
    let f = setup();
    let provider = Address::generate(&f.env);
    let patient = bref(&f.env, 7);

    for id in 1..=60u64 {
        f.client.mint(
            &f.minter, &id, &patient, &provider, &101u32, &10i128, &0i128, &10i128,
        );
    }

    // A limit above MAX_RECEIPTS_PAGE is capped.
    let first = f.client.receipts_of_provider(&provider, &0u32, &u32::MAX);
    assert_eq!(first.len(), MAX_RECEIPTS_PAGE);
    assert_eq!(first.get(0).unwrap().voucher_id, 1);
    assert_eq!(first.get(49).unwrap().voucher_id, 50);

    // The next page continues in mint order.
    let second = f.client.receipts_of_provider(&provider, &50u32, &50u32);
    assert_eq!(second.len(), 10);
    assert_eq!(second.get(0).unwrap().voucher_id, 51);
    assert_eq!(second.get(9).unwrap().voucher_id, 60);

    // Zero, a start past the end, and an exact end are all empty.
    assert_eq!(
        f.client.receipts_of_provider(&provider, &0u32, &0u32).len(),
        0
    );
    assert_eq!(
        f.client
            .receipts_of_provider(&provider, &60u32, &10u32)
            .len(),
        0
    );
    assert_eq!(
        f.client
            .receipts_of_provider(&provider, &999u32, &10u32)
            .len(),
        0
    );
}

#[test]
fn provider_index_skips_a_corrupted_entry() {
    // The index is written by `mint` and should always be complete. If a
    // bug ever leaves a hole, one bad entry must not make the rest of a
    // clinic's history unreadable.
    let f = setup();
    let provider = Address::generate(&f.env);
    let patient = bref(&f.env, 7);
    f.client.mint(
        &f.minter, &1u64, &patient, &provider, &101u32, &10i128, &0i128, &10i128,
    );
    f.client.mint(
        &f.minter, &2u64, &patient, &provider, &101u32, &20i128, &0i128, &20i128,
    );

    let contract = f.client.address.clone();
    f.env.as_contract(&contract, || {
        f.env
            .storage()
            .persistent()
            .remove(&DataKey::ProviderReceipt(provider.clone(), 0));
    });

    let receipts = f.client.receipts_of_provider(&provider, &0u32, &10u32);
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts.get(0).unwrap().voucher_id, 2);
}

#[test]
fn missing_receipt_errors() {
    let f = setup();
    let err = f.client.try_get_receipt(&404u64).err().unwrap().unwrap();
    assert_eq!(err, ReceiptError::ReceiptNotFound);
}

#[test]
fn name_symbol_and_receipt_uri_are_wallet_metadata() {
    let f = setup();
    assert_eq!(
        f.client.name(),
        String::from_str(&f.env, "SCI Care Receipt")
    );
    assert_eq!(f.client.symbol(), String::from_str(&f.env, "SCIREC"));
    assert_eq!(
        f.client.receipt_uri(&1u64),
        String::from_str(&f.env, "https://receipts.sci.health/v1/receipt/1")
    );
    assert_eq!(
        f.client.receipt_uri(&42u64),
        String::from_str(&f.env, "https://receipts.sci.health/v1/receipt/42")
    );
}

#[test]
fn receipt_uri_contains_no_patient_data() {
    let f = setup();
    let patient = bref(&f.env, 7);
    let provider = Address::generate(&f.env);
    f.client.mint(
        &f.minter, &1u64, &patient, &provider, &101u32, &10i128, &0i128, &10i128,
    );

    let uri = f.client.receipt_uri(&1u64);
    // Derived from the voucher id only: no patient data, no beneficiary ref.
    assert_eq!(
        uri,
        String::from_str(&f.env, "https://receipts.sci.health/v1/receipt/1")
    );
}

#[test]
fn no_transfer_entry_points() {
    for case in AUTH_MATRIX {
        assert!(
            case.function != "transfer" && case.function != "transfer_from",
            "`{}` would make receipts transferable; receipts are soulbound",
            case.function,
        );
    }
}

#[test]
fn minter_handover_takes_two_steps() {
    let f = setup();
    let new_minter = Address::generate(&f.env);
    f.client.propose_minter(&f.admin, &new_minter);
    assert_eq!(f.client.get_minter(), f.minter);
    assert_eq!(f.client.get_pending_minter(), Some(new_minter.clone()));

    f.client.accept_minter(&new_minter);
    assert_eq!(f.client.get_minter(), new_minter);
    assert_eq!(f.client.get_pending_minter(), None);

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
            &0i128,
            &10i128,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, ReceiptError::NotAuthorized);
}

#[test]
fn non_admin_cannot_propose_minter() {
    let f = setup();
    let impostor = Address::generate(&f.env);
    let err = f
        .client
        .try_propose_minter(&impostor, &impostor)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, ReceiptError::NotAuthorized);
}

#[test]
fn only_the_nominee_can_accept_minter() {
    let f = setup();
    let nominee = Address::generate(&f.env);

    // Nothing proposed yet.
    let err = f.client.try_accept_minter(&nominee).err().unwrap().unwrap();
    assert_eq!(err, ReceiptError::NoPendingMinter);

    f.client.propose_minter(&f.admin, &nominee);
    let intruder = Address::generate(&f.env);
    let err = f
        .client
        .try_accept_minter(&intruder)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, ReceiptError::NotAuthorized);

    // The failed acceptance did not consume the nomination.
    f.client.accept_minter(&nominee);
    assert_eq!(f.client.get_minter(), nominee);
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

    // The old admin can no longer propose a minter.
    let err = f
        .client
        .try_propose_minter(&f.admin, &new_admin)
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

// ----- authorisation matrix -----
//
// One row per public function: the exact signer it must demand, and nothing
// else. The host records one entry per address a `require_auth()` was made
// for during an invocation, so comparing `env.auths()` with the expected
// signer catches both kinds of mistake:
//
//   * a missing `require_auth()` — anyone could mint a receipt, and
//   * an extra `require_auth()` — the voucher contract could not mint one at
//     settlement, because a contract address cannot produce a signature.
//
// The table in `docs/developers/contracts.md` is the rendered form of this
// same list, and `authorisation_table_in_the_docs_matches_the_matrix` fails
// when the two drift.
//
// What breaks if this is wrong: a receipt is the patient's proof that care was
// paid for. If `mint` accepts anyone, that proof is worthless.

extern crate std;

use soroban_sdk::testutils::storage::Persistent as _;
use soroban_sdk::testutils::{Events as _, Ledger as _};
use soroban_sdk::xdr::{ContractEvent, ContractEventBody, ScVal};

use crate::DataKey;

const DAY_IN_LEDGERS: u32 = 17_280;
const NINETY_DAYS: u32 = 90 * DAY_IN_LEDGERS;

/// A plain fixture plus the address the `accept_admin` row needs. It is
/// generated here rather than in `setup()` so every other test keeps the same
/// generated addresses — and therefore the same snapshots.
struct AuthFixture {
    f: Fixture,
    /// Never used as admin: the nominee for the `accept_admin` row.
    new_admin: Address,
    /// Never used as minter: the nominee for the `accept_minter` row.
    new_minter: Address,
}

impl AuthFixture {
    fn new() -> Self {
        let f = setup();
        let new_admin = Address::generate(&f.env);
        let new_minter = Address::generate(&f.env);
        Self {
            f,
            new_admin,
            new_minter,
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
    /// No signature at all; the note says why (a view, a one-shot setup).
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

fn minter_addr(f: &AuthFixture) -> Address {
    f.f.minter.clone()
}

fn new_admin_addr(f: &AuthFixture) -> Address {
    f.new_admin.clone()
}

fn new_minter_addr(f: &AuthFixture) -> Address {
    f.new_minter.clone()
}

/// Every public function of the receipt book, in declaration order.
const AUTH_MATRIX: &[AuthCase] = &[
    AuthCase {
        function: "initialize",
        auth: Auth::Free("none (one-shot)"),
    },
    AuthCase {
        function: "propose_minter",
        auth: signer("`admin`", admin_addr),
    },
    AuthCase {
        function: "accept_minter",
        auth: signer("`new_minter`", new_minter_addr),
    },
    AuthCase {
        function: "mint",
        auth: signer("`minter`", minter_addr),
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
        function: "get_pending_admin",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "get_pending_minter",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "get_receipt",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "get_receipts",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "count_for",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "provider_stats",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "receipts_of_provider",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "name",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "symbol",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "receipt_uri",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "get_minter",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "get_admin",
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

fn mint_one(f: &Fixture) {
    f.client.mint(
        &f.minter,
        &1u64,
        &bref(&f.env, 7),
        &Address::generate(&f.env),
        &101u32,
        &30_000_000i128,
        &0i128,
        &30_000_000i128,
    );
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
    let client = &f.f.client;
    let env = &f.f.env;

    match function {
        // One-shot setup needs a fresh contract, so it cannot share the
        // fixture: assert on an env that has never been initialised.
        "initialize" => {
            let env = Env::default();
            env.mock_all_auths();
            let a = Address::generate(&env);
            let address = env.register(ReceiptBook, ());
            ReceiptBookClient::new(&env, &address).initialize(&a, &a);
            return case_run(&env);
        }
        "propose_minter" => {
            client.propose_minter(&f.f.admin, &f.new_minter);
        }
        "accept_minter" => {
            client.propose_minter(&f.f.admin, &f.new_minter);
            client.accept_minter(&f.new_minter);
        }
        "mint" => {
            mint_one(&f.f);
        }
        "propose_admin" => {
            client.propose_admin(&f.f.admin, &f.new_admin);
        }
        "accept_admin" => {
            client.propose_admin(&f.f.admin, &f.new_admin);
            client.accept_admin(&f.new_admin);
        }
        "get_pending_admin" => {
            client.get_pending_admin();
        }
        "get_pending_minter" => {
            client.get_pending_minter();
        }
        "get_receipt" => {
            mint_one(&f.f);
            client.get_receipt(&1u64);
        }
        "get_receipts" => {
            mint_one(&f.f);
            client.get_receipts(&soroban_sdk::Vec::from_array(env, [1u64, 404u64]));
        }
        "count_for" => {
            client.count_for(&bref(env, 7));
        }
        "provider_stats" => {
            client.provider_stats(&f.f.minter);
        }
        "receipts_of_provider" => {
            client.receipts_of_provider(&f.f.minter, &0u32, &10u32);
        }
        "name" => {
            client.name();
        }
        "symbol" => {
            client.symbol();
        }
        "receipt_uri" => {
            client.receipt_uri(&1u64);
        }
        "get_minter" => {
            client.get_minter();
        }
        "get_admin" => {
            client.get_admin();
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
                 a missing require_auth() would let anyone mint, and an extra one \
                 would stop the voucher contract minting at settlement",
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
            "`{}` is documented as {note}, but it demanded {} signature(s)",
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

    let rows = doc_rows(DOCS, "receipt");
    assert_eq!(
        rows.len(),
        AUTH_MATRIX.len(),
        "docs/developers/contracts.md documents {} receipt functions but the matrix has {}",
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
        "storage reads or writes changed for a receipt entry point. If the change is \
         intentional, run `just storage-counts` and explain the new cost in the pull \
         request; if it is not, the call is doing more ledger work than it used to",
    );
}

// ----- event snapshots -----
//
// Events are an API for the indexer in `sci-healthcare-backend`, which decodes
// them by event name and field name. These tests read the decoded XDR event the
// contract publishes and pin both the topic list and the data map.
//
// What breaks if this is wrong: a renamed field turns into `undefined` in the
// indexer, and the patient's care history silently loses a column.

/// The single event published by the last invocation, from `contract`.
fn one_event(env: &Env, contract: &Address) -> ContractEvent {
    let events = env.events().all().filter_by_contract(contract);
    let list = events.events();
    assert_eq!(
        list.len(),
        1,
        "expected the invocation to publish exactly one receipt event, got {:?}",
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
            .unwrap_or_else(|_| panic!("event field `{name}` is not a usable address")),
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

/// Asserts the event name (topic 0), the extra topics, and the exact set of
/// data fields — the layout an indexer decodes.
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
    let minter = Address::generate(&env);
    let address = env.register(ReceiptBook, ());
    ReceiptBookClient::new(&env, &address).initialize(&admin, &minter);

    let e = one_event(&env, &address);
    assert_layout(&e, "initialized", &[], &["admin"]);
    assert_eq!(field_address(&env, &e, "admin"), admin);
}

#[test]
fn minter_proposed_event_snapshot() {
    let f = setup();
    let new_minter = Address::generate(&f.env);

    f.client.propose_minter(&f.admin, &new_minter);

    let e = one_event(&f.env, &f.client.address);
    assert_layout(
        &e,
        "minter_proposed",
        &[ScVal::from(&f.admin)],
        &["pending_minter"],
    );
    assert_eq!(field_address(&f.env, &e, "pending_minter"), new_minter);
}

#[test]
fn minter_changed_event_snapshot() {
    let f = setup();
    let new_minter = Address::generate(&f.env);

    f.client.propose_minter(&f.admin, &new_minter);
    f.client.accept_minter(&new_minter);

    let e = one_event(&f.env, &f.client.address);
    // `minter` is data, not a topic: an indexer reading it from the topic list
    // gets undefined, so the layout is pinned here.
    assert_layout(&e, "minter_changed", &[], &["minter"]);
    assert_eq!(field_address(&f.env, &e, "minter"), new_minter);
}

#[test]
fn receipt_minted_event_snapshot() {
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
        &0i128,
        &30_000_000i128,
    );

    let e = one_event(&f.env, &f.client.address);
    assert_layout(
        &e,
        "receipt_minted",
        &[ScVal::from(&patient), ScVal::from(&provider)],
        &["voucher_id", "service_code", "amount", "settled_at"],
    );
    assert_eq!(field_u64(&e, "voucher_id"), 1);
    assert_eq!(field_u32(&e, "service_code"), 101);
    assert_eq!(field_i128(&e, "amount"), 30_000_000);
    assert_eq!(field_u64(&e, "settled_at"), 0);
}

#[test]
fn admin_proposed_event_snapshot() {
    let f = setup();
    let nominee = Address::generate(&f.env);

    f.client.propose_admin(&f.admin, &nominee);

    let e = one_event(&f.env, &f.client.address);
    assert_layout(
        &e,
        "admin_proposed",
        &[ScVal::from(&f.admin)],
        &["pending_admin"],
    );
    assert_eq!(field_address(&f.env, &e, "pending_admin"), nominee);
}

#[test]
fn admin_changed_event_snapshot() {
    let f = setup();
    let new_admin = Address::generate(&f.env);
    f.client.propose_admin(&f.admin, &new_admin);

    f.client.accept_admin(&new_admin);

    let e = one_event(&f.env, &f.client.address);
    assert_layout(&e, "admin_changed", &[], &["new_admin"]);
    assert_eq!(field_address(&f.env, &e, "new_admin"), new_admin);
}

// ----- storage lifetime (TTL) -----
//
// A receipt is the patient's proof that care was paid for. It is only worth
// anything if it outlives the voucher, so these tests jump the ledger past the
// contract's own thresholds and prove the entries are still there.
//
// What breaks if this is wrong: a receipt that lapses has to be restored before
// it can be read, and a patient asking for their history pays for that.

fn advance_days(f: &Fixture, days: u32) {
    f.env.ledger().with_mut(|li| {
        li.sequence_number += days * DAY_IN_LEDGERS;
        li.timestamp += u64::from(days) * 24 * 60 * 60;
    });
}

fn receipt_ttl(f: &Fixture, voucher_id: u64) -> u32 {
    let contract = f.client.address.clone();
    let key = DataKey::Receipt(voucher_id);
    f.env
        .as_contract(&contract, || f.env.storage().persistent().get_ttl(&key))
}

fn count_ttl(f: &Fixture, beneficiary: &BytesN<32>) -> u32 {
    let contract = f.client.address.clone();
    let key = DataKey::Count(beneficiary.clone());
    f.env
        .as_contract(&contract, || f.env.storage().persistent().get_ttl(&key))
}

fn provider_count_ttl(f: &Fixture, provider: &Address) -> u32 {
    let contract = f.client.address.clone();
    let key = DataKey::ProviderCount(provider.clone());
    f.env
        .as_contract(&contract, || f.env.storage().persistent().get_ttl(&key))
}

fn provider_entry_ttl(f: &Fixture, provider: &Address, index: u32) -> u32 {
    let contract = f.client.address.clone();
    let key = DataKey::ProviderReceipt(provider.clone(), index);
    f.env
        .as_contract(&contract, || f.env.storage().persistent().get_ttl(&key))
}

fn provider_total_ttl(f: &Fixture, provider: &Address) -> u32 {
    let contract = f.client.address.clone();
    let key = DataKey::ProviderTotal(provider.clone());
    f.env
        .as_contract(&contract, || f.env.storage().persistent().get_ttl(&key))
}

#[test]
fn receipt_survives_sixty_days_of_activity() {
    let f = setup();
    let provider = Address::generate(&f.env);
    let patient = bref(&f.env, 7);
    f.client.mint(
        &f.minter,
        &1u64,
        &patient,
        &provider,
        &101u32,
        &30_000_000,
        &0i128,
        &30_000_000,
    );

    // Ten days at a time, the way the app lists a patient's history.
    for day in 1..=6 {
        advance_days(&f, 10);
        let r = f.client.get_receipt(&1u64);
        assert_eq!(
            r.amount,
            30_000_000,
            "the receipt was archived on day {}",
            day * 10
        );
        assert_eq!(r.provider, provider);
        assert_eq!(f.client.count_for(&patient), 1);
    }

    // Day 60: the read above crossed the 30-day threshold and pushed the
    // entry back out to a full 90 days.
    assert_eq!(receipt_ttl(&f, 1), NINETY_DAYS);
}

#[test]
fn receipt_read_restores_a_lapsed_entry() {
    let f = setup();
    let provider = Address::generate(&f.env);
    let patient = bref(&f.env, 7);
    f.client.mint(
        &f.minter,
        &1u64,
        &patient,
        &provider,
        &101u32,
        &30_000_000,
        &0i128,
        &30_000_000,
    );

    // 95 days of silence: past the 90-day TTL the mint set on the receipt.
    advance_days(&f, 95);

    // The host restores the lapsed entry on the first read; the contract's
    // read path then extends it again. Nothing is lost.
    assert_eq!(f.client.get_receipt(&1u64).amount, 30_000_000);
    assert_eq!(receipt_ttl(&f, 1), NINETY_DAYS);
    assert_eq!(f.client.count_for(&patient), 1);
}

#[test]
fn count_for_extends_its_own_entry() {
    let f = setup();
    let provider = Address::generate(&f.env);
    let patient = bref(&f.env, 7);
    f.client.mint(
        &f.minter,
        &1u64,
        &patient,
        &provider,
        &101u32,
        &30_000_000,
        &0i128,
        &30_000_000,
    );

    // Day 61: the counter has crossed the 30-day threshold. Only a read of
    // `count_for` can extend it — the receipt's own TTL does not cover it.
    advance_days(&f, 61);
    assert_eq!(f.client.count_for(&patient), 1);
    assert_eq!(count_ttl(&f, &patient), NINETY_DAYS);

    // And an unknown beneficiary is still an ordinary zero, not an error.
    advance_days(&f, 30);
    assert_eq!(f.client.count_for(&bref(&f.env, 9)), 0);
}

#[test]
fn provider_index_extends_its_own_entries() {
    let f = setup();
    let provider = Address::generate(&f.env);
    let patient = bref(&f.env, 7);
    f.client.mint(
        &f.minter,
        &1u64,
        &patient,
        &provider,
        &101u32,
        &30_000_000,
        &0i128,
        &30_000_000,
    );

    // Day 61: the index entries are past their 30-day threshold. Only a
    // read through `receipts_of_provider` extends them.
    advance_days(&f, 61);
    let receipts = f.client.receipts_of_provider(&provider, &0u32, &10u32);
    assert_eq!(receipts.len(), 1);
    assert_eq!(provider_count_ttl(&f, &provider), NINETY_DAYS);
    assert_eq!(provider_entry_ttl(&f, &provider, 0), NINETY_DAYS);
}

#[test]
fn provider_stats_extend_their_own_entries() {
    let f = setup();
    let provider = Address::generate(&f.env);
    let patient = bref(&f.env, 7);
    f.client.mint(
        &f.minter,
        &1u64,
        &patient,
        &provider,
        &101u32,
        &30_000_000,
        &0i128,
        &30_000_000,
    );

    // Day 61: the counters are past their 30-day threshold. Only a read of
    // `provider_stats` extends them — the receipt's own TTL does not cover
    // them, and neither does `receipts_of_provider` extend the total.
    advance_days(&f, 61);
    assert_eq!(f.client.provider_stats(&provider), (1, 30_000_000));
    assert_eq!(provider_count_ttl(&f, &provider), NINETY_DAYS);
    assert_eq!(provider_total_ttl(&f, &provider), NINETY_DAYS);

    // An unknown provider is still an ordinary (0, 0), not an error.
    assert_eq!(f.client.provider_stats(&Address::generate(&f.env)), (0, 0));
}

#[test]
fn not_initialized_errors() {
    let env = Env::default();
    env.mock_all_auths();
    let _a = Address::generate(&env);
    let _patient = BytesN::from_array(&env, &[1u8; 32]);
    let addr = env.register(ReceiptBook, ());
    let client = ReceiptBookClient::new(&env, &addr);

    // Calls that read the admin/minter before initialize should return NotInitialized.
    let err = client.try_get_admin().err().unwrap().unwrap();
    assert_eq!(err, ReceiptError::NotInitialized);
    let err = client.try_get_minter().err().unwrap().unwrap();
    assert_eq!(err, ReceiptError::NotInitialized);
}

// ----- event table in the docs -----
//
// Events are the indexer's API. This test reads the `#[contractevent]` structs
// in `lib.rs` and compares the event names, topics and data fields with the
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
    const SOURCE: &str = include_str!("lib.rs");

    let source = events_in_source(SOURCE);
    let rows = doc_event_rows(DOCS, "receipt");
    assert_eq!(
        rows.len(),
        source.len(),
        "docs/developers/contracts.md documents {} receipt events but the source has {}",
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
                     to the receipt event table with topics {:?} and data {:?}",
                    event.name, event.topics, event.data,
                )
            });
        assert_eq!(
            split_doc_fields(&row[1]),
            event.topics,
            "the topics documented for `{}` do not match the #[topic] fields in \
             contracts/receipt/src/lib.rs",
            event.name,
        );
        assert_eq!(
            split_doc_fields(&row[2]),
            event.data,
            "the data fields documented for `{}` do not match the non-topic fields in \
             contracts/receipt/src/lib.rs. The indexer decodes them by name \
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
// pointed at existing state) loses access to receipts already in the ledger.

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
    let provider = Address::from_str(
        &env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    );

    // The fixture is the named layout the release before fee/net recorded,
    // and must keep decoding: `fee = 0`, `net = amount`.
    check_stored_type(
        &env,
        "receipt",
        crate::storage::StoredReceiptV1 {
            voucher_id: 42,
            beneficiary_ref: bref(&env, 7),
            provider: provider.clone(),
            service_code: 101,
            amount: 30_000_000,
            settled_at: 1_700_000_000,
        },
    );

    // The current positional layout, pinned so a later field change has to be
    // made deliberately and with a migration.
    check_stored_type(
        &env,
        "receipt_v2",
        crate::storage::StoredReceipt::from_receipt(&crate::Receipt {
            voucher_id: 42,
            beneficiary_ref: bref(&env, 7),
            provider,
            service_code: 101,
            amount: 30_000_000,
            fee: 300_000,
            net: 29_700_000,
            settled_at: 1_700_000_000,
        }),
    );
}

#[test]
fn receipt_with_an_unexpected_layout_is_not_found() {
    // A stored value that is neither a current nor a legacy receipt must not
    // be decoded into one: report it missing, exactly like an absent entry.
    let f = setup();
    let contract = f.client.address.clone();
    f.env.as_contract(&contract, || {
        let bad = soroban_sdk::Vec::<soroban_sdk::Val>::from_array(
            &f.env,
            [soroban_sdk::Val::from(1u32)],
        );
        f.env
            .storage()
            .persistent()
            .set(&DataKey::Receipt(1u64), &bad);
    });

    let err = f.client.try_get_receipt(&1u64).err().unwrap().unwrap();
    assert_eq!(err, ReceiptError::ReceiptNotFound);
}

#[test]
fn old_format_receipt_still_decodes() {
    let f = setup();
    let provider = Address::generate(&f.env);
    let id = 42u64;

    // Exactly what the previous release wrote: a named receipt without the
    // split. It must read back, with the gross amount as the net.
    let old = crate::storage::StoredReceiptV1 {
        voucher_id: id,
        beneficiary_ref: bref(&f.env, 7),
        provider: provider.clone(),
        service_code: 101,
        amount: 30_000_000,
        settled_at: 1_700_000_000,
    };
    let contract = f.client.address.clone();
    f.env.as_contract(&contract, || {
        f.env
            .storage()
            .persistent()
            .set(&DataKey::Receipt(id), &old);
    });

    let r = f.client.get_receipt(&id);
    assert_eq!(r.voucher_id, id);
    assert_eq!(r.provider, provider);
    assert_eq!(r.service_code, 101);
    assert_eq!(r.amount, 30_000_000);
    assert_eq!(r.settled_at, 1_700_000_000);
    assert_eq!(r.fee, 0, "the old layout never recorded a fee");
    assert_eq!(r.net, 30_000_000, "the gross amount is all it knew");
    assert_eq!(r.net + r.fee, r.amount);

    // And the batch view reads it the same way.
    let receipts = f
        .client
        .get_receipts(&soroban_sdk::Vec::from_array(&f.env, [id]));
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts.get(0).unwrap().net, 30_000_000);
}
