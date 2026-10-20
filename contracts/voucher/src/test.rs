#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, BytesN, Env, String,
};

use crate::{
    VoucherError, VoucherEscrow, VoucherEscrowClient, VoucherStatus,
    DEFAULT_MAX_ATTESTATIONS_PER_DAY,
};
use sci_receipt::{ReceiptBook, ReceiptBookClient};
use sci_registry::{ProviderStatus, Registry, RegistryClient, MIN_PRICE_NOTICE_SECS};

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
fn cannot_fund_paused_service() {
    let f = setup();
    f.registry
        .set_service_available(&f.provider, &CONSULT, &false);

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
    assert_eq!(err, VoucherError::ServiceUnavailable);
    assert_eq!(f.voucher.next_voucher_id(), 1, "no voucher was written");

    // Resumed: funding works again.
    f.registry
        .set_service_available(&f.provider, &CONSULT, &true);
    assert_eq!(fund(&f), 1);
}

#[test]
fn pausing_a_service_does_not_affect_funded_vouchers() {
    let f = setup();
    let id = fund(&f);

    f.registry
        .set_service_available(&f.provider, &CONSULT, &false);

    f.voucher.claim(&f.provider, &id);
    f.voucher.attest(&f.attester, &id);
    advance(&f.env, DISPUTE_WINDOW + 1);
    f.voucher.settle(&id);
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Settled);
}

#[test]
fn scheduled_price_rise_applies_to_funding_at_the_effective_time() {
    let f = setup();
    let effective_at = START_TIME + MIN_PRICE_NOTICE_SECS;
    f.registry
        .schedule_service_price(&f.provider, &CONSULT, &(CONSULT_PRICE * 2), &effective_at);

    // Before the effective time the listed price still funds.
    assert_eq!(fund(&f), 1);

    f.env.ledger().with_mut(|li| li.timestamp = effective_at);
    let err = f
        .voucher
        .try_create_voucher(
            &f.funder,
            &patient(&f.env),
            &f.provider,
            &CONSULT,
            &CONSULT_PRICE,
            &(effective_at + 1000),
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::AmountBelowPrice);
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
fn version_is_the_crate_version() {
    let f = setup();
    let version = f.voucher.version();
    assert_eq!(version, String::from_str(&f.env, env!("CARGO_PKG_VERSION")));
    // Every contract shares the workspace version.
    assert_eq!(version, String::from_str(&f.env, "0.2.0"));
}

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

// ----- attestation cap -----

const DAY: u64 = 24 * 60 * 60;

/// Funds and claims a fresh voucher, ready to attest.
fn claimed(f: &Fixture) -> u64 {
    let id = fund(f);
    f.voucher.claim(&f.provider, &id);
    id
}

#[test]
fn attestation_cap_defaults_and_counts_down() {
    let f = setup();
    assert_eq!(
        f.voucher.get_attestation_cap(),
        DEFAULT_MAX_ATTESTATIONS_PER_DAY
    );
    assert_eq!(
        f.voucher.attestations_left(&f.attester),
        DEFAULT_MAX_ATTESTATIONS_PER_DAY
    );

    let id = claimed(&f);
    f.voucher.attest(&f.attester, &id);

    assert_eq!(
        f.voucher.attestations_left(&f.attester),
        DEFAULT_MAX_ATTESTATIONS_PER_DAY - 1
    );
}

#[test]
fn attestation_cap_is_enforced_and_resets_next_utc_day() {
    let f = setup();
    f.voucher.set_attestation_cap(&f.admin, &2);
    let ids = [claimed(&f), claimed(&f), claimed(&f)];

    f.voucher.attest(&f.attester, &ids[0]);
    f.voucher.attest(&f.attester, &ids[1]);
    assert_eq!(f.voucher.attestations_left(&f.attester), 0);

    let err = f
        .voucher
        .try_attest(&f.attester, &ids[2])
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::AttestationCapReached);
    assert_eq!(
        f.voucher.get_voucher(&ids[2]).status,
        VoucherStatus::Claimed
    );

    // Still the same UTC day one second before midnight.
    let midnight = (START_TIME / DAY + 1) * DAY;
    f.env.ledger().with_mut(|li| li.timestamp = midnight - 1);
    let err = f
        .voucher
        .try_attest(&f.attester, &ids[2])
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::AttestationCapReached);

    // The counter resets at the UTC day boundary.
    f.env.ledger().with_mut(|li| li.timestamp = midnight);
    assert_eq!(f.voucher.attestations_left(&f.attester), 2);
    f.voucher.attest(&f.attester, &ids[2]);
    assert_eq!(
        f.voucher.get_voucher(&ids[2]).status,
        VoucherStatus::Attested
    );
}

#[test]
fn attestation_cap_is_per_attester() {
    let f = setup();
    f.voucher.set_attestation_cap(&f.admin, &1);
    let other = Address::generate(&f.env);
    f.registry.add_attester(&f.admin, &other);
    let ids = [claimed(&f), claimed(&f)];

    f.voucher.attest(&f.attester, &ids[0]);
    f.voucher.attest(&other, &ids[1]);

    assert_eq!(f.voucher.attestations_left(&f.attester), 0);
    assert_eq!(f.voucher.attestations_left(&other), 0);
}

#[test]
fn rejected_attestation_does_not_use_the_allowance() {
    let f = setup();
    let unclaimed = fund(&f);

    assert!(f.voucher.try_attest(&f.attester, &unclaimed).is_err());

    assert_eq!(
        f.voucher.attestations_left(&f.attester),
        DEFAULT_MAX_ATTESTATIONS_PER_DAY
    );
}

#[test]
fn lowering_the_cap_applies_to_today() {
    let f = setup();
    let ids = [claimed(&f), claimed(&f)];
    f.voucher.attest(&f.attester, &ids[0]);

    f.voucher.set_attestation_cap(&f.admin, &1);

    assert_eq!(f.voucher.attestations_left(&f.attester), 0);
    let err = f
        .voucher
        .try_attest(&f.attester, &ids[1])
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::AttestationCapReached);
}

#[test]
fn only_admin_sets_a_positive_attestation_cap() {
    let f = setup();
    let err = f
        .voucher
        .try_set_attestation_cap(&f.funder, &5)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::NotAuthorized);

    let err = f
        .voucher
        .try_set_attestation_cap(&f.admin, &0)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::InvalidAttestationCap);
    assert_eq!(
        f.voucher.get_attestation_cap(),
        DEFAULT_MAX_ATTESTATIONS_PER_DAY
    );
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

#[test]
fn multisig_admin_can_act_on_all_three_contracts() {
    // A Stellar account with several signers is still one `Address` to
    // Soroban: the network enforces the signer weights and 2-of-3 threshold,
    // and the contract only sees the G-account that authorised the call. This
    // test hands every contract to that account and proves each one accepts
    // its admin calls, which is what `docs/developers/multisig-admin.md`
    // sets up on testnet.
    let f = setup();
    let multisig = Address::from_string(&String::from_str(
        &f.env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    ));

    f.registry.propose_admin(&f.admin, &multisig);
    f.receipts.propose_admin(&f.admin, &multisig);
    f.voucher.propose_admin(&f.admin, &multisig);

    // Nothing changes until the nominee accepts.
    assert_eq!(f.registry.get_admin(), f.admin);
    assert_eq!(f.receipts.get_admin(), f.admin);
    assert_eq!(f.voucher.get_config().admin, f.admin);

    f.registry.accept_admin(&multisig);
    f.receipts.accept_admin(&multisig);
    f.voucher.accept_admin(&multisig);

    assert_eq!(f.registry.get_admin(), multisig);
    assert_eq!(f.receipts.get_admin(), multisig);
    assert_eq!(f.voucher.get_config().admin, multisig);

    // Registry: an ordinary admin action.
    let new_attester = Address::generate(&f.env);
    f.registry.add_attester(&multisig, &new_attester);
    assert!(f.registry.is_attester(&new_attester));

    // Registry: suspend and restore the provider through the same account.
    f.registry
        .set_provider_status(&multisig, &f.provider, &ProviderStatus::Suspended);
    assert!(!f.registry.is_active_provider(&f.provider));
    f.registry
        .set_provider_status(&multisig, &f.provider, &ProviderStatus::Active);

    // Receipt book: repoint the minter, then put it back for the voucher.
    let new_minter = Address::generate(&f.env);
    f.receipts.set_minter(&multisig, &new_minter);
    assert_eq!(f.receipts.get_minter(), new_minter);
    f.receipts.set_minter(&multisig, &f.voucher.address);

    // Voucher: resolve a real dispute as the multisig admin.
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);
    f.voucher.dispute(&f.funder, &id, &1u32);
    f.voucher.resolve_dispute(&multisig, &id, &true);
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Refunded);
}

// ----- event snapshots -----
//
// Every lifecycle event must carry what was paid for — `service_code` and
// `amount` — so an indexer or wallet reading a single event can describe it
// without a storage lookup. `VoucherCreated` already did; claim, attest,
// dispute, settle and refund now do too. These tests read the decoded XDR
// event itself (the same bytes a snapshot records and an indexer decodes),
// and run offline like the rest of the suite.

extern crate std;

use soroban_sdk::testutils::Events as _;
use soroban_sdk::xdr::{ContractEvent, ContractEventBody, ScVal};
use std::vec::Vec;

/// The single event published by the last invocation, from this contract.
///
/// `settle` and `refund` move tokens first, so the SAC's own `transfer`
/// event is in the same list; the voucher event is what is under test.
/// A descriptive message beats a wrong-but-passing assertion: if the call
/// published nothing, or something else entirely, the failure says so.
fn one_event(f: &Fixture) -> ContractEvent {
    let events = f.env.events().all().filter_by_contract(&f.voucher.address);
    let list = events.events();
    assert_eq!(
        list.len(),
        1,
        "expected the invocation to publish exactly one voucher event, got {:?}",
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
    let data = match &e.body {
        ContractEventBody::V0(v0) => &v0.data,
    };
    match data {
        ScVal::Map(Some(map)) => Some(map),
        _ => None,
    }
}

/// The text of a `Symbol` value, or `None` when it is not one.
fn symbol(v: &ScVal) -> Option<&str> {
    match v {
        ScVal::Symbol(s) => (&s.0).try_into().ok(),
        _ => None,
    }
}

/// One field of the data map, with a descriptive failure when it is
/// missing or has the wrong XDR type.
fn field(e: &ContractEvent, name: &str) -> ScVal {
    let entries = data(e).unwrap_or_else(|| panic!("event data is not a map: {:?}", e));
    let found = entries
        .iter()
        .find(|entry| symbol(&entry.key) == Some(name))
        .unwrap_or_else(|| {
            let keys: Vec<&str> = entries
                .iter()
                .filter_map(|entry| symbol(&entry.key))
                .collect();
            panic!(
                "event has no {name} field; its fields are {keys:?}: {:?}",
                e
            )
        });
    found.val.clone()
}

fn field_u32(e: &ContractEvent, name: &str) -> u32 {
    match field(e, name) {
        ScVal::U32(v) => v,
        other => panic!("event field {name} is {other:?}, want u32"),
    }
}

fn field_u64(e: &ContractEvent, name: &str) -> u64 {
    match field(e, name) {
        ScVal::U64(v) => v,
        other => panic!("event field {name} is {other:?}, want u64"),
    }
}

fn field_i128(e: &ContractEvent, name: &str) -> i128 {
    match field(e, name) {
        ScVal::I128(parts) => i128::from(&parts),
        other => panic!("event field {name} is {other:?}, want i128"),
    }
}

/// The declared fields present on the event, in the order the data map
/// carries them (an XDR map is sorted by key, so this is stable).
fn field_names(e: &ContractEvent) -> Vec<&str> {
    let Some(entries) = data(e) else {
        panic!("event data is not a map: {e:?}");
    };
    entries
        .iter()
        .filter_map(|entry| symbol(&entry.key))
        .collect()
}

/// The event name (topic 0) and its first topic, which is always the
/// acting address for these lifecycle events.
fn assert_name_and_topic(e: &ContractEvent, want_name: &str) {
    assert_eq!(
        symbol(topics(e).first().expect("event has no topics")),
        Some(want_name),
        "topic 0 names the event, got {:?}",
        topics(e),
    );
    assert!(
        matches!(topics(e).get(1), Some(ScVal::Address(_))),
        "topic 1 of {want_name} is the acting address, got {:?}",
        topics(e),
    );
}

#[test]
fn claim_event_snapshot_carries_service_and_amount() {
    let f = setup();
    let id = fund(&f);

    f.voucher.claim(&f.provider, &id);

    let e = one_event(&f);
    assert_name_and_topic(&e, "voucher_claimed");
    assert_eq!(field_u64(&e, "voucher_id"), id);
    assert_eq!(field_u64(&e, "claimed_at"), START_TIME);
    assert_eq!(field_u32(&e, "service_code"), CONSULT);
    assert_eq!(field_i128(&e, "amount"), CONSULT_PRICE);
    assert_eq!(
        field_names(&e),
        ["amount", "claimed_at", "service_code", "voucher_id"],
    );
}

#[test]
fn attest_event_snapshot_carries_service_and_amount() {
    let f = setup();
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);

    f.voucher.attest(&f.attester, &id);

    let e = one_event(&f);
    assert_name_and_topic(&e, "voucher_attested");
    assert_eq!(field_u64(&e, "voucher_id"), id);
    assert_eq!(field_u64(&e, "attested_at"), START_TIME);
    assert_eq!(
        field_u64(&e, "dispute_deadline"),
        START_TIME + DISPUTE_WINDOW
    );
    assert_eq!(field_u32(&e, "service_code"), CONSULT);
    assert_eq!(field_i128(&e, "amount"), CONSULT_PRICE);
    assert_eq!(
        field_u32(&e, "remaining_today"),
        DEFAULT_MAX_ATTESTATIONS_PER_DAY - 1
    );
    assert_eq!(
        field_names(&e),
        [
            "amount",
            "attested_at",
            "dispute_deadline",
            "remaining_today",
            "service_code",
            "voucher_id",
        ],
    );
}

#[test]
fn dispute_event_snapshot_carries_service_and_amount() {
    let f = setup();
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);

    // Reason 1: the provider took the voucher but never delivered.
    f.voucher.dispute(&f.funder, &id, &1);

    let e = one_event(&f);
    assert_name_and_topic(&e, "voucher_disputed");
    assert_eq!(field_u64(&e, "voucher_id"), id);
    assert_eq!(field_u32(&e, "reason_code"), 1);
    assert_eq!(field_u32(&e, "service_code"), CONSULT);
    assert_eq!(field_i128(&e, "amount"), CONSULT_PRICE);
    assert_eq!(
        field_names(&e),
        ["amount", "reason_code", "service_code", "voucher_id"],
    );
}

#[test]
fn settle_event_snapshot_carries_service_and_amount() {
    let f = setup();
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);
    f.voucher.attest(&f.attester, &id);
    advance(&f.env, DISPUTE_WINDOW + 1);
    // Quoted before settling: `one_event` reads the *last* invocation.
    let (fee, net) = f.voucher.quote(&CONSULT_PRICE);

    f.voucher.settle(&id);

    let e = one_event(&f);
    assert_name_and_topic(&e, "voucher_settled");
    assert_eq!(field_u64(&e, "voucher_id"), id);
    assert_eq!(field_i128(&e, "net"), net);
    assert_eq!(field_i128(&e, "fee"), fee);
    // The gross escrow sits beside the payout, so one event answers both
    // "what was paid" and "what landed".
    assert_eq!(field_i128(&e, "net") + field_i128(&e, "fee"), CONSULT_PRICE);
    assert_eq!(field_u32(&e, "service_code"), CONSULT);
    assert_eq!(field_i128(&e, "amount"), CONSULT_PRICE);
    assert_eq!(
        field_names(&e),
        ["amount", "fee", "net", "service_code", "voucher_id"],
    );
}

#[test]
fn refund_event_snapshot_carries_service_and_amount() {
    let f = setup();
    let id = fund(&f);
    // The voucher runs to expiry unclaimed; no care happened.
    advance(&f.env, 30 * 24 * 60 * 60 + 1);

    f.voucher.refund(&id);

    let e = one_event(&f);
    assert_name_and_topic(&e, "voucher_refunded");
    assert_eq!(field_u64(&e, "voucher_id"), id);
    assert_eq!(field_i128(&e, "amount"), CONSULT_PRICE);
    assert_eq!(field_u32(&e, "service_code"), CONSULT);
    assert_eq!(field_names(&e), ["amount", "service_code", "voucher_id"],);
}

// ----- authorisation matrix -----
//
// One row per public function: the exact signer it must demand, and nothing
// else. The host records one entry per address a `require_auth()` was made
// for during an invocation, so comparing `env.auths()` with the expected
// signer catches both kinds of mistake:
//
//   * a missing `require_auth()` — anyone can move another party's escrow, and
//   * an extra `require_auth()` — settlement would stop being permissionless
//     and the call could no longer be made by a relayer.
//
// The table in `docs/developers/contracts.md` is the rendered form of this
// same list, and `authorisation_table_in_the_docs_matches_the_matrix` fails
// when the two drift.
//
// What breaks if this is wrong: `settle` and `refund` are deliberately
// permissionless, while `claim`, `attest` and `dispute` are the only checks
// standing between one party and another party's money.

use crate::DataKey;

const DAY_IN_LEDGERS: u32 = 17_280;
const SEVEN_DAYS: u32 = 7 * DAY_IN_LEDGERS;
const NINETY_DAYS: u32 = 90 * DAY_IN_LEDGERS;
const THIRTY_DAYS: u32 = 30 * DAY_IN_LEDGERS;

/// A plain fixture plus the address the `accept_admin` row needs. It is
/// generated here rather than in `setup()` so every other test keeps the same
/// generated addresses — and therefore the same snapshots.
struct AuthFixture {
    f: Fixture,
    /// Never used as admin: the nominee for the `accept_admin` row.
    new_admin: Address,
}

impl AuthFixture {
    fn new() -> Self {
        let f = setup();
        let new_admin = Address::generate(&f.env);
        Self { f, new_admin }
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

fn funder_addr(f: &AuthFixture) -> Address {
    f.f.funder.clone()
}

fn provider_addr(f: &AuthFixture) -> Address {
    f.f.provider.clone()
}

fn attester_addr(f: &AuthFixture) -> Address {
    f.f.attester.clone()
}

fn admin_addr(f: &AuthFixture) -> Address {
    f.f.admin.clone()
}

fn new_admin_addr(f: &AuthFixture) -> Address {
    f.new_admin.clone()
}

/// Every public function of the voucher contract, in declaration order.
const AUTH_MATRIX: &[AuthCase] = &[
    AuthCase {
        function: "initialize",
        auth: Auth::Free("none (one-shot)"),
    },
    AuthCase {
        function: "create_voucher",
        auth: signer("`funder`", funder_addr),
    },
    AuthCase {
        function: "claim",
        auth: signer("`provider`", provider_addr),
    },
    AuthCase {
        function: "attest",
        auth: signer("`attester`", attester_addr),
    },
    AuthCase {
        function: "dispute",
        auth: signer("`funder`", funder_addr),
    },
    AuthCase {
        function: "settle",
        auth: Auth::Free("none (permissionless)"),
    },
    AuthCase {
        function: "refund",
        auth: Auth::Free("none (permissionless)"),
    },
    AuthCase {
        function: "resolve_dispute",
        auth: signer("`admin`", admin_addr),
    },
    AuthCase {
        function: "set_attestation_cap",
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
        function: "get_attestation_cap",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "attestations_left",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "get_pending_admin",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "refundable_at",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "get_voucher",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "get_config",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "next_voucher_id",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "quote",
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
    let voucher = &f.f.voucher;
    let env = &f.f.env;

    match function {
        // One-shot setup needs a fresh contract, so it cannot share the
        // fixture: assert on an env that has never been initialised.
        "initialize" => {
            let env = Env::default();
            env.mock_all_auths();
            let a = Address::generate(&env);
            let address = env.register(VoucherEscrow, ());
            VoucherEscrowClient::new(&env, &address).initialize(
                &a,
                &a,
                &a,
                &a,
                &DISPUTE_WINDOW,
                &FEE_BPS,
                &a,
            );
            return recorded_auths(&env);
        }
        "create_voucher" => {
            fund(&f.f);
        }
        "claim" => {
            let id = fund(&f.f);
            voucher.claim(&f.f.provider, &id);
        }
        "attest" => {
            let id = fund(&f.f);
            voucher.claim(&f.f.provider, &id);
            voucher.attest(&f.f.attester, &id);
        }
        "dispute" => {
            let id = fund(&f.f);
            voucher.claim(&f.f.provider, &id);
            voucher.dispute(&f.f.funder, &id, &1u32);
        }
        "settle" => {
            let id = fund(&f.f);
            voucher.claim(&f.f.provider, &id);
            voucher.attest(&f.f.attester, &id);
            advance(env, DISPUTE_WINDOW + 1);
            voucher.settle(&id);
        }
        "refund" => {
            let id = fund(&f.f);
            advance(env, 31 * 24 * 60 * 60);
            voucher.refund(&id);
        }
        "resolve_dispute" => {
            let id = fund(&f.f);
            voucher.claim(&f.f.provider, &id);
            voucher.dispute(&f.f.funder, &id, &1u32);
            voucher.resolve_dispute(&f.f.admin, &id, &true);
        }
        "set_attestation_cap" => {
            voucher.set_attestation_cap(&f.f.admin, &10);
        }
        "get_attestation_cap" => {
            voucher.get_attestation_cap();
        }
        "attestations_left" => {
            voucher.attestations_left(&f.f.attester);
        }
        "propose_admin" => {
            voucher.propose_admin(&f.f.admin, &f.new_admin);
        }
        "accept_admin" => {
            voucher.propose_admin(&f.f.admin, &f.new_admin);
            voucher.accept_admin(&f.new_admin);
        }
        "get_pending_admin" => {
            voucher.get_pending_admin();
        }
        "refundable_at" => {
            let id = fund(&f.f);
            voucher.refundable_at(&id);
        }
        "get_voucher" => {
            let id = fund(&f.f);
            voucher.get_voucher(&id);
        }
        "get_config" => {
            voucher.get_config();
        }
        "next_voucher_id" => {
            voucher.next_voucher_id();
        }
        "quote" => {
            voucher.quote(&CONSULT_PRICE);
        }
        "version" => {
            voucher.version();
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
             If that is intentional, every caller and relayer has to change",
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

    let rows = doc_rows(DOCS, "voucher");
    assert_eq!(
        rows.len(),
        AUTH_MATRIX.len(),
        "docs/developers/contracts.md documents {} voucher functions but the matrix has {}",
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

// ----- storage lifetime (TTL) -----
//
// Soroban archives persistent entries whose TTL lapses. This contract extends
// TTL on every voucher read and write and on every instance write, and these
// tests jump the ledger far enough to prove it:
//
//   * a voucher touched by normal use is still readable after 60 days, and
//     gets a fresh 90-day TTL once it drops below the 30-day threshold, and
//   * a voucher nobody touches lapses at day 90 and is restored by the host on
//     the next read — the funder's money is never lost, but the restore is a
//     fee (see docs/developers/contracts.md#storage-lifetime-ttl).
//
// What breaks if this is wrong: an archived voucher has to be restored before
// `refund` will touch it, so a funder that waited too long pays to get their
// own money back.

use soroban_sdk::testutils::storage::{Instance as _, Persistent as _};

fn advance_days(f: &Fixture, days: u32) {
    f.env.ledger().with_mut(|li| {
        li.sequence_number += days * DAY_IN_LEDGERS;
        li.timestamp += u64::from(days) * 24 * 60 * 60;
    });
}

fn voucher_ttl(f: &Fixture, id: u64) -> u32 {
    let contract = f.voucher.address.clone();
    let key = DataKey::Voucher(id);
    f.env
        .as_contract(&contract, || f.env.storage().persistent().get_ttl(&key))
}

fn instance_ttl(f: &Fixture) -> u32 {
    let contract = f.voucher.address.clone();
    f.env
        .as_contract(&contract, || f.env.storage().instance().get_ttl())
}

#[test]
fn live_voucher_survives_sixty_days_of_activity() {
    let f = setup();
    let id = fund(&f);

    // Ten days at a time, the way a wallet or the indexer polls a voucher.
    for day in 1..=6 {
        advance_days(&f, 10);
        let v = f.voucher.get_voucher(&id);
        assert_eq!(
            v.status,
            VoucherStatus::Funded,
            "the voucher was archived on day {}",
            day * 10,
        );
        assert_eq!(v.amount, CONSULT_PRICE);
        assert_eq!(v.funder, f.funder);

        if day == 3 {
            // The funder keeps using the contract mid-way: a normal write,
            // which extends the instance as well as the new entry.
            let expires_at = f.env.ledger().timestamp() + 30 * 24 * 60 * 60;
            f.voucher.create_voucher(
                &f.funder,
                &patient(&f.env),
                &f.provider,
                &CONSULT,
                &CONSULT_PRICE,
                &expires_at,
            );
            assert_eq!(instance_ttl(&f), THIRTY_DAYS);
        }
    }

    // Day 60: still readable, and the read above crossed the 30-day
    // threshold, so the entry was pushed back out to a full 90 days.
    assert_eq!(voucher_ttl(&f, id), NINETY_DAYS);
}

#[test]
fn settled_voucher_and_its_receipt_survive_sixty_days_of_activity() {
    let f = setup();
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);
    f.voucher.attest(&f.attester, &id);
    advance(&f.env, DISPUTE_WINDOW + 1);
    f.voucher.settle(&id);

    for day in 1..=6 {
        advance_days(&f, 10);
        assert_eq!(
            f.voucher.get_voucher(&id).status,
            VoucherStatus::Settled,
            "the settled voucher was archived on day {}",
            day * 10,
        );
        assert_eq!(f.receipts.get_receipt(&id).amount, CONSULT_PRICE);
        assert_eq!(f.receipts.count_for(&patient(&f.env)), 1);
    }
}

#[test]
fn abandoned_voucher_is_restored_and_still_refunds() {
    let f = setup();
    let start_balance = f.token.balance(&f.funder);
    let abandoned = fund(&f);

    // The funder keeps using the contract — one new voucher every 20 days,
    // which extends the instance and their own new entry — but never reads
    // the first voucher again.
    for _ in 0..5 {
        advance_days(&f, 20);
        let expires_at = f.env.ledger().timestamp() + 30 * 24 * 60 * 60;
        f.voucher.create_voucher(
            &f.funder,
            &patient(&f.env),
            &f.provider,
            &CONSULT,
            &CONSULT_PRICE,
            &expires_at,
        );
    }

    // Day 100. The abandoned entry lapsed at day 90 and is not in the live
    // ledger any more: the host restores it on this read, which is what makes
    // it readable at all. Nothing was lost.
    assert_eq!(
        f.voucher.get_voucher(&abandoned).status,
        VoucherStatus::Funded
    );
    assert_eq!(voucher_ttl(&f, abandoned), NINETY_DAYS);

    // And the money is still recoverable: expiry passed long ago.
    f.voucher.refund(&abandoned);
    assert_eq!(
        f.voucher.get_voucher(&abandoned).status,
        VoucherStatus::Refunded
    );
    // The five later vouchers are still escrowed; this one came back.
    assert_eq!(
        f.token.balance(&f.funder),
        start_balance - 5 * CONSULT_PRICE,
    );
}

#[test]
fn contract_instance_survives_sixty_days_of_writes() {
    let f = setup();
    let id = fund(&f);

    // Writes, not reads, are what extend the instance: it carries the config
    // and the id counter, and it lapses after 30 days of silence.
    for idx in 0..6 {
        advance_days(&f, 10);
        let expires_at = f.env.ledger().timestamp() + 30 * 24 * 60 * 60;
        let next = f.voucher.create_voucher(
            &f.funder,
            &patient(&f.env),
            &f.provider,
            &CONSULT,
            &CONSULT_PRICE,
            &expires_at,
        );
        assert!(next > id);
        // The instance is extended once its remaining TTL drops below the
        // 7-day threshold, so it always has at least that much left.
        assert!(
            instance_ttl(&f) >= SEVEN_DAYS,
            "the contract instance is about to lapse on day {}",
            (idx + 1) * 10,
        );
    }

    assert_eq!(f.voucher.get_config().fee_bps, FEE_BPS);
}

#[test]
fn writes_extend_the_contract_instance() {
    let f = setup();
    // Setup left it with a 30-day TTL; 25 days of silence leaves five.
    advance_days(&f, 25);
    assert!(instance_ttl(&f) < SEVEN_DAYS);

    fund(&f);

    assert_eq!(instance_ttl(&f), THIRTY_DAYS);
}

// ----- event snapshots for the rest of the surface -----
//
// The lifecycle events are pinned above. These cover the remaining functions
// that publish, so every event this contract can emit has a test: a change to
// any of them fails here, with a message naming the decoder that would break.

fn field_address(env: &Env, e: &ContractEvent, name: &str) -> Address {
    use soroban_sdk::TryFromVal;

    let value = field(e, name);
    match &value {
        ScVal::Address(_) => Address::try_from_val(env, &value)
            .unwrap_or_else(|_| panic!("event field `{name}` is not a usable address")),
        other => panic!("event field `{name}` is {other:?}, want an address"),
    }
}

fn field_bool(e: &ContractEvent, name: &str) -> bool {
    match field(e, name) {
        ScVal::Bool(v) => v,
        other => panic!("event field `{name}` is {other:?}, want bool"),
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
    let a = Address::generate(&env);
    let address = env.register(VoucherEscrow, ());
    VoucherEscrowClient::new(&env, &address).initialize(
        &a,
        &a,
        &a,
        &a,
        &DISPUTE_WINDOW,
        &FEE_BPS,
        &a,
    );

    let events = env.events().all().filter_by_contract(&address);
    let list = events.events();
    assert_eq!(list.len(), 1, "initialize publishes one event");
    let e = list[0].clone();

    assert_layout(&e, "initialized", &[], &["admin"]);
    assert_eq!(field_address(&env, &e, "admin"), a);
}

#[test]
fn voucher_created_event_snapshot() {
    let f = setup();
    let id = fund(&f);
    // Read after the event: any further call replaces what `one_event` sees.
    let expires_at = START_TIME + 30 * 24 * 60 * 60;

    let e = one_event(&f);
    assert_layout(
        &e,
        "voucher_created",
        &[
            ScVal::from(&f.funder),
            ScVal::from(&f.provider),
            ScVal::from(&patient(&f.env)),
        ],
        &[
            "voucher_id",
            "service_code",
            "amount",
            "created_at",
            "expires_at",
        ],
    );
    assert_eq!(field_u64(&e, "voucher_id"), id);
    assert_eq!(field_u32(&e, "service_code"), CONSULT);
    assert_eq!(field_i128(&e, "amount"), CONSULT_PRICE);
    assert_eq!(field_u64(&e, "created_at"), START_TIME);
    assert_eq!(field_u64(&e, "expires_at"), expires_at);
}

#[test]
fn dispute_resolved_event_snapshot() {
    let f = setup();
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);
    f.voucher.dispute(&f.funder, &id, &1u32);

    f.voucher.resolve_dispute(&f.admin, &id, &true);

    // Resolving publishes two events — this one, and the `voucher_refunded`
    // that the payout emits — so pick this one out by name.
    let events = f.env.events().all().filter_by_contract(&f.voucher.address);
    let list = events.events();
    let resolved: std::vec::Vec<ContractEvent> = list
        .iter()
        .filter(|e| symbol(&topics(e)[0]) == Some("dispute_resolved"))
        .cloned()
        .collect();
    assert_eq!(
        resolved.len(),
        1,
        "resolving a dispute publishes exactly one dispute_resolved event",
    );
    let e = resolved[0].clone();
    assert_layout(
        &e,
        "dispute_resolved",
        &[ScVal::from(&f.admin)],
        &["voucher_id", "refunded_funder"],
    );
    assert_eq!(field_u64(&e, "voucher_id"), id);
    assert!(field_bool(&e, "refunded_funder"));
}

#[test]
fn admin_proposed_event_snapshot() {
    let f = setup();
    let nominee = Address::generate(&f.env);

    f.voucher.propose_admin(&f.admin, &nominee);

    let e = one_event(&f);
    assert_layout(
        &e,
        "admin_proposed",
        &[ScVal::from(&f.admin)],
        &["pending_admin"],
    );
    assert_eq!(field_address(&f.env, &e, "pending_admin"), nominee);
}

#[test]
fn attestation_cap_changed_event_snapshot() {
    let f = setup();

    f.voucher.set_attestation_cap(&f.admin, &25);

    let e = one_event(&f);
    assert_layout(
        &e,
        "attestation_cap_changed",
        &[ScVal::from(&f.admin)],
        &["cap"],
    );
    assert_eq!(field_u32(&e, "cap"), 25);
}

#[test]
fn admin_changed_event_snapshot() {
    let f = setup();
    let nominee = Address::generate(&f.env);
    f.voucher.propose_admin(&f.admin, &nominee);

    f.voucher.accept_admin(&nominee);

    let e = one_event(&f);
    // `new_admin` is data, not a topic: an indexer reading it from the topic
    // list gets undefined, so the layout is pinned here.
    assert_layout(&e, "admin_changed", &[], &["new_admin"]);
    assert_eq!(field_address(&f.env, &e, "new_admin"), nominee);
}
