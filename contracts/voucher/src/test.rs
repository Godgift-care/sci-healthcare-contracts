#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, BytesN, Env, String,
};

use crate::{VoucherError, VoucherEscrow, VoucherEscrowClient, VoucherStatus};
use sci_receipt::{ReceiptBook, ReceiptBookClient};
use sci_registry::{ProviderStatus, Registry, RegistryClient};

const DISPUTE_WINDOW: u64 = 72 * 60 * 60; // 72 hours
const FEE_BPS: u32 = 100; // 1%
const START_TIME: u64 = 1_700_000_000;
const CONSULT: u32 = 101;
const CONSULT_PRICE: i128 = 30_000_000; // 3.00 USDC at 7 decimals

struct Fixture {
    env: Env,
    voucher: VoucherEscrowClient<'static>,
    registry: RegistryClient<'static>,
    receipts: ReceiptBookClient<'static>,
    token: token::Client<'static>,
    admin: Address,
    funder: Address,
    provider: Address,
    attester: Address,
    fee_account: Address,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = START_TIME);

    let admin = Address::generate(&env);
    let funder = Address::generate(&env);
    let provider = Address::generate(&env);
    let attester = Address::generate(&env);
    let fee_account = Address::generate(&env);

    // Settlement token, standing in for USDC on Stellar.
    let issuer = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(issuer);
    let token_addr = sac.address();
    let token_client = token::Client::new(&env, &token_addr);
    let token_admin = token::StellarAssetClient::new(&env, &token_addr);
    token_admin.mint(&funder, &1_000_000_000i128);

    let registry_addr = env.register(Registry, ());
    let registry = RegistryClient::new(&env, &registry_addr);
    registry.initialize(&admin);

    let receipt_addr = env.register(ReceiptBook, ());
    let receipts = ReceiptBookClient::new(&env, &receipt_addr);
    // Minter is repointed at the voucher contract below.
    receipts.initialize(&admin, &admin);

    let voucher_addr = env.register(VoucherEscrow, ());
    let voucher = VoucherEscrowClient::new(&env, &voucher_addr);
    voucher.initialize(
        &admin,
        &registry_addr,
        &receipt_addr,
        &token_addr,
        &DISPUTE_WINDOW,
        &FEE_BPS,
        &fee_account,
    );
    receipts.set_minter(&admin, &voucher_addr);

    // An active provider with one priced service, and one attester.
    registry.register_provider(
        &provider,
        &String::from_str(&env, "Ikeja General Clinic"),
        &String::from_str(&env, "NG"),
    );
    registry.set_provider_status(&admin, &provider, &ProviderStatus::Active);
    registry.upsert_service(
        &provider,
        &CONSULT,
        &String::from_str(&env, "Outpatient consult"),
        &CONSULT_PRICE,
    );
    registry.add_attester(&admin, &attester);

    Fixture {
        env,
        voucher,
        registry,
        receipts,
        token: token_client,
        admin,
        funder,
        provider,
        attester,
        fee_account,
    }
}

fn patient(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[9u8; 32])
}

fn advance(env: &Env, secs: u64) {
    env.ledger().with_mut(|li| li.timestamp += secs);
}

fn fund(f: &Fixture) -> u64 {
    f.voucher.create_voucher(
        &f.funder,
        &patient(&f.env),
        &f.provider,
        &CONSULT,
        &CONSULT_PRICE,
        &(START_TIME + 30 * 24 * 60 * 60),
    )
}

// ----- funding -----

#[test]
fn create_voucher_escrows_funds() {
    let f = setup();
    let before = f.token.balance(&f.funder);

    let id = fund(&f);

    assert_eq!(id, 1);
    assert_eq!(f.token.balance(&f.funder), before - CONSULT_PRICE);

    let v = f.voucher.get_voucher(&id);
    assert_eq!(v.status, VoucherStatus::Funded);
    assert_eq!(v.amount, CONSULT_PRICE);
    assert_eq!(v.provider, f.provider);
    assert_eq!(v.funder, f.funder);
}

#[test]
fn voucher_ids_increment() {
    let f = setup();
    assert_eq!(fund(&f), 1);
    assert_eq!(fund(&f), 2);
    assert_eq!(f.voucher.next_voucher_id(), 3);
}

#[test]
fn funder_need_not_be_beneficiary() {
    // A relative abroad funds care for someone at home. Same code path.
    let f = setup();
    let id = fund(&f);
    let v = f.voucher.get_voucher(&id);
    assert_eq!(v.funder, f.funder);
    assert_eq!(v.beneficiary_ref, patient(&f.env));
}

#[test]
fn cannot_fund_inactive_provider() {
    let f = setup();
    f.registry
        .set_provider_status(&f.admin, &f.provider, &ProviderStatus::Suspended);

    let err = f
        .voucher
        .try_create_voucher(
            &f.funder,
            &patient(&f.env),
            &f.provider,
            &CONSULT,
            &CONSULT_PRICE,
            &(START_TIME + 1000),
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::ProviderNotActive);
}

#[test]
fn cannot_fund_unoffered_service() {
    let f = setup();
    let err = f
        .voucher
        .try_create_voucher(
            &f.funder,
            &patient(&f.env),
            &f.provider,
            &999u32,
            &CONSULT_PRICE,
            &(START_TIME + 1000),
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::ServiceNotOffered);
}

#[test]
fn cannot_underfund_below_list_price() {
    let f = setup();
    let err = f
        .voucher
        .try_create_voucher(
            &f.funder,
            &patient(&f.env),
            &f.provider,
            &CONSULT,
            &(CONSULT_PRICE - 1),
            &(START_TIME + 1000),
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::AmountBelowPrice);
}

#[test]
fn rejects_non_positive_amount() {
    let f = setup();
    let err = f
        .voucher
        .try_create_voucher(
            &f.funder,
            &patient(&f.env),
            &f.provider,
            &CONSULT,
            &0i128,
            &(START_TIME + 1000),
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::InvalidAmount);
}

#[test]
fn rejects_expiry_in_the_past() {
    let f = setup();
    let err = f
        .voucher
        .try_create_voucher(
            &f.funder,
            &patient(&f.env),
            &f.provider,
            &CONSULT,
            &CONSULT_PRICE,
            &(START_TIME - 1),
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::InvalidExpiry);
}

// ----- happy path -----

#[test]
fn full_lifecycle_pays_provider_and_mints_receipt() {
    let f = setup();
    let id = fund(&f);

    f.voucher.claim(&f.provider, &id);
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Claimed);

    f.voucher.attest(&f.attester, &id);
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Attested);

    advance(&f.env, DISPUTE_WINDOW + 1);
    f.voucher.settle(&id);

    let expected_fee = CONSULT_PRICE * FEE_BPS as i128 / 10_000;
    let expected_net = CONSULT_PRICE - expected_fee;

    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Settled);
    assert_eq!(f.token.balance(&f.provider), expected_net);
    assert_eq!(f.token.balance(&f.fee_account), expected_fee);

    // Care receipt exists and carries no clinical data.
    let r = f.receipts.get_receipt(&id);
    assert_eq!(r.provider, f.provider);
    assert_eq!(r.service_code, CONSULT);
    assert_eq!(r.amount, CONSULT_PRICE);
    assert_eq!(f.receipts.count_for(&patient(&f.env)), 1);
}

#[test]
fn quote_matches_settlement() {
    let f = setup();
    let (fee, net) = f.voucher.quote(&CONSULT_PRICE);
    assert_eq!(fee + net, CONSULT_PRICE);

    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);
    f.voucher.attest(&f.attester, &id);
    advance(&f.env, DISPUTE_WINDOW + 1);
    f.voucher.settle(&id);

    assert_eq!(f.token.balance(&f.provider), net);
    assert_eq!(f.token.balance(&f.fee_account), fee);
}

#[test]
fn settlement_is_permissionless() {
    // A provider must not depend on the funder or an operator to get paid.
    let f = setup();
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);
    f.voucher.attest(&f.attester, &id);
    advance(&f.env, DISPUTE_WINDOW + 1);

    // No auth context of any particular party is required.
    f.voucher.settle(&id);
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Settled);
}

// ----- claim -----

#[test]
fn only_named_provider_can_claim() {
    let f = setup();
    let id = fund(&f);
    let impostor = Address::generate(&f.env);

    let err = f.voucher.try_claim(&impostor, &id).err().unwrap().unwrap();
    assert_eq!(err, VoucherError::NotAuthorized);
}

#[test]
fn cannot_claim_expired_voucher() {
    let f = setup();
    let id = fund(&f);
    advance(&f.env, 31 * 24 * 60 * 60);

    let err = f
        .voucher
        .try_claim(&f.provider, &id)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::VoucherExpired);
}

#[test]
fn suspended_provider_cannot_claim() {
    let f = setup();
    let id = fund(&f);
    f.registry
        .set_provider_status(&f.admin, &f.provider, &ProviderStatus::Suspended);

    let err = f
        .voucher
        .try_claim(&f.provider, &id)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::ProviderNotActive);
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Funded);
}

#[test]
fn cannot_claim_twice() {
    let f = setup();
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);

    let err = f
        .voucher
        .try_claim(&f.provider, &id)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::InvalidStatus);
}

// ----- attestation -----

#[test]
fn provider_cannot_attest_its_own_payment() {
    let f = setup();
    // Even if the provider somehow holds an attester role.
    f.registry.add_attester(&f.admin, &f.provider);

    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);

    let err = f
        .voucher
        .try_attest(&f.provider, &id)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::NotAuthorized);
}

#[test]
fn non_attester_cannot_attest() {
    let f = setup();
    let stranger = Address::generate(&f.env);
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);

    let err = f.voucher.try_attest(&stranger, &id).err().unwrap().unwrap();
    assert_eq!(err, VoucherError::NotAuthorized);
}

#[test]
fn revoked_attester_cannot_attest() {
    let f = setup();
    f.registry.remove_attester(&f.admin, &f.attester);
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);

    let err = f
        .voucher
        .try_attest(&f.attester, &id)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::NotAuthorized);
}

#[test]
fn cannot_attest_unclaimed_voucher() {
    let f = setup();
    let id = fund(&f);

    let err = f
        .voucher
        .try_attest(&f.attester, &id)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::InvalidStatus);
}

// ----- settlement guards -----

#[test]
fn cannot_settle_during_dispute_window() {
    let f = setup();
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);
    f.voucher.attest(&f.attester, &id);

    let err = f.voucher.try_settle(&id).err().unwrap().unwrap();
    assert_eq!(err, VoucherError::DisputeWindowOpen);
}

#[test]
fn cannot_settle_unattested_voucher() {
    let f = setup();
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);
    advance(&f.env, DISPUTE_WINDOW + 1);

    let err = f.voucher.try_settle(&id).err().unwrap().unwrap();
    assert_eq!(err, VoucherError::InvalidStatus);
}

#[test]
fn cannot_settle_twice() {
    let f = setup();
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);
    f.voucher.attest(&f.attester, &id);
    advance(&f.env, DISPUTE_WINDOW + 1);
    f.voucher.settle(&id);

    let err = f.voucher.try_settle(&id).err().unwrap().unwrap();
    assert_eq!(err, VoucherError::InvalidStatus);
}

// ----- refund -----

#[test]
fn expired_unclaimed_voucher_refunds_in_full() {
    let f = setup();
    let before = f.token.balance(&f.funder);
    let id = fund(&f);
    advance(&f.env, 31 * 24 * 60 * 60);

    f.voucher.refund(&id);

    // No fee is taken on care that never happened.
    assert_eq!(f.token.balance(&f.funder), before);
    assert_eq!(f.token.balance(&f.fee_account), 0);
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Refunded);
}

#[test]
fn cannot_refund_before_expiry() {
    let f = setup();
    let id = fund(&f);

    let err = f.voucher.try_refund(&id).err().unwrap().unwrap();
    assert_eq!(err, VoucherError::NotYetExpired);
}

#[test]
fn claimed_voucher_is_locked_during_claim_grace() {
    // One day past expiry: the attester still has time to confirm care
    // that happened just before the voucher ran out.
    let f = setup();
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);
    advance(&f.env, 31 * 24 * 60 * 60);

    let err = f.voucher.try_refund(&id).err().unwrap().unwrap();
    assert_eq!(err, VoucherError::NotYetExpired);
}

#[test]
fn claimed_but_never_attested_voucher_refunds_after_grace() {
    // A claim alone must never lock the funder's money forever.
    let f = setup();
    let before = f.token.balance(&f.funder);
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);

    let expires_at = f.voucher.get_voucher(&id).expires_at;
    assert_eq!(
        f.voucher.refundable_at(&id),
        expires_at + crate::CLAIM_GRACE_SECS
    );

    f.env
        .ledger()
        .with_mut(|li| li.timestamp = expires_at + crate::CLAIM_GRACE_SECS);
    f.voucher.refund(&id);

    assert_eq!(f.token.balance(&f.funder), before);
    assert_eq!(f.token.balance(&f.provider), 0);
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Refunded);
}

#[test]
fn attested_voucher_cannot_be_refunded() {
    let f = setup();
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);
    f.voucher.attest(&f.attester, &id);
    advance(&f.env, 365 * 24 * 60 * 60);

    let err = f.voucher.try_refund(&id).err().unwrap().unwrap();
    assert_eq!(err, VoucherError::InvalidStatus);
    let err = f.voucher.try_refundable_at(&id).err().unwrap().unwrap();
    assert_eq!(err, VoucherError::InvalidStatus);
}

#[test]
fn refundable_at_is_expiry_for_unclaimed_voucher() {
    let f = setup();
    let id = fund(&f);
    assert_eq!(
        f.voucher.refundable_at(&id),
        f.voucher.get_voucher(&id).expires_at
    );
}

// ----- disputes -----

#[test]
fn funder_disputes_claimed_but_undelivered_care() {
    let f = setup();
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);

    f.voucher.dispute(&f.funder, &id, &1u32);
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Disputed);
}

#[test]
fn funder_disputes_within_window() {
    let f = setup();
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);
    f.voucher.attest(&f.attester, &id);
    advance(&f.env, DISPUTE_WINDOW - 10);

    f.voucher.dispute(&f.funder, &id, &2u32);
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Disputed);
}

#[test]
fn cannot_dispute_after_window_closes() {
    let f = setup();
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);
    f.voucher.attest(&f.attester, &id);
    advance(&f.env, DISPUTE_WINDOW + 1);

    let err = f
        .voucher
        .try_dispute(&f.funder, &id, &2u32)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::DisputeWindowClosed);
}

#[test]
fn only_funder_can_dispute() {
    let f = setup();
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);
    let stranger = Address::generate(&f.env);

    let err = f
        .voucher
        .try_dispute(&stranger, &id, &1u32)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::NotAuthorized);
}

#[test]
fn resolving_for_funder_refunds() {
    let f = setup();
    let before = f.token.balance(&f.funder);
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);
    f.voucher.dispute(&f.funder, &id, &1u32);

    f.voucher.resolve_dispute(&f.admin, &id, &true);

    assert_eq!(f.token.balance(&f.funder), before);
    assert_eq!(f.token.balance(&f.provider), 0);
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Refunded);
}

#[test]
fn resolving_for_provider_pays_out() {
    let f = setup();
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);
    f.voucher.dispute(&f.funder, &id, &1u32);

    f.voucher.resolve_dispute(&f.admin, &id, &false);

    let expected_fee = CONSULT_PRICE * FEE_BPS as i128 / 10_000;
    assert_eq!(f.token.balance(&f.provider), CONSULT_PRICE - expected_fee);
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Settled);
    // A settled dispute still produces a care receipt.
    assert_eq!(f.receipts.count_for(&patient(&f.env)), 1);
}

#[test]
fn non_admin_cannot_resolve() {
    let f = setup();
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);
    f.voucher.dispute(&f.funder, &id, &1u32);
    let impostor = Address::generate(&f.env);

    let err = f
        .voucher
        .try_resolve_dispute(&impostor, &id, &true)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::NotAuthorized);
}

#[test]
fn cannot_resolve_undisputed_voucher() {
    let f = setup();
    let id = fund(&f);

    let err = f
        .voucher
        .try_resolve_dispute(&f.admin, &id, &true)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::InvalidStatus);
}

// ----- config -----

#[test]
fn initialize_is_single_shot() {
    let f = setup();
    let a = Address::generate(&f.env);
    let err = f
        .voucher
        .try_initialize(&a, &a, &a, &a, &DISPUTE_WINDOW, &FEE_BPS, &a)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::AlreadyInitialized);
}

#[test]
fn excessive_fee_rejected_at_init() {
    let env = Env::default();
    env.mock_all_auths();
    let a = Address::generate(&env);
    let addr = env.register(VoucherEscrow, ());
    let client = VoucherEscrowClient::new(&env, &addr);

    let err = client
        .try_initialize(&a, &a, &a, &a, &DISPUTE_WINDOW, &1_001u32, &a)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::InvalidFee);
}

#[test]
fn zero_fee_pays_provider_in_full() {
    let f = setup();
    // Config is fixed at init, so verify the maths directly.
    let (fee, net) = f.voucher.quote(&CONSULT_PRICE);
    assert_eq!(fee, CONSULT_PRICE / 100);
    assert_eq!(net, CONSULT_PRICE - fee);
}

#[test]
fn missing_voucher_errors() {
    let f = setup();
    let err = f.voucher.try_get_voucher(&404u64).err().unwrap().unwrap();
    assert_eq!(err, VoucherError::VoucherNotFound);
}

// ----- administration -----

#[test]
fn admin_handover_takes_two_steps() {
    let f = setup();
    let new_admin = Address::generate(&f.env);

    f.voucher.propose_admin(&f.admin, &new_admin);
    assert_eq!(f.voucher.get_config().admin, f.admin);
    assert_eq!(f.voucher.get_pending_admin(), Some(new_admin.clone()));

    f.voucher.accept_admin(&new_admin);
    assert_eq!(f.voucher.get_config().admin, new_admin);
    assert_eq!(f.voucher.get_pending_admin(), None);
}

#[test]
fn new_admin_resolves_disputes_and_old_admin_cannot() {
    let f = setup();
    let new_admin = Address::generate(&f.env);
    f.voucher.propose_admin(&f.admin, &new_admin);
    f.voucher.accept_admin(&new_admin);

    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);
    f.voucher.dispute(&f.funder, &id, &1u32);

    let err = f
        .voucher
        .try_resolve_dispute(&f.admin, &id, &true)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::NotAuthorized);

    f.voucher.resolve_dispute(&new_admin, &id, &true);
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Refunded);
}

#[test]
fn only_the_nominee_can_accept_admin() {
    let f = setup();
    let nominee = Address::generate(&f.env);
    let intruder = Address::generate(&f.env);
    f.voucher.propose_admin(&f.admin, &nominee);

    let err = f
        .voucher
        .try_accept_admin(&intruder)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::NotAuthorized);
}

#[test]
fn accept_admin_without_proposal_errors() {
    let f = setup();
    let someone = Address::generate(&f.env);
    let err = f.voucher.try_accept_admin(&someone).err().unwrap().unwrap();
    assert_eq!(err, VoucherError::NoPendingAdmin);
}

#[test]
fn non_admin_cannot_propose_admin() {
    let f = setup();
    let err = f
        .voucher
        .try_propose_admin(&f.funder, &f.funder)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::NotAuthorized);
}
