#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, IssuerFlags, Ledger as _, StellarAssetContract},
    token, Address, Bytes, BytesN, Env, String,
};

use crate::{
    Exposure, VoucherError, VoucherEscrow, VoucherEscrowClient, VoucherOptions, VoucherStatus,
    DEFAULT_DISPUTE_WINDOW_BOUNDS, DEFAULT_MAX_ATTESTATIONS_PER_DAY,
};
use sci_receipt::{ReceiptBook, ReceiptBookClient};
use sci_registry::{ProviderStatus, Registry, RegistryClient, MIN_PRICE_NOTICE_SECS};

const DISPUTE_WINDOW: u64 = 72 * 60 * 60; // 72 hours
const FEE_BPS: u32 = 100; // 1%
/// Half the protocol fee (50 of the 100 fee basis points) goes to the
/// attester in the default fixture. `Fee account receives` tests allow for it.
const ATTESTER_SHARE_BPS: u32 = 50;
const START_TIME: u64 = 1_700_000_000;
const CONSULT: u32 = 101;
const CONSULT_PRICE: i128 = 30_000_000; // 3.00 USDC at 7 decimals

struct Fixture {
    env: Env,
    voucher: VoucherEscrowClient<'static>,
    registry: RegistryClient<'static>,
    receipts: ReceiptBookClient<'static>,
    token: token::Client<'static>,
    /// The settlement token's issuer side, for clawback and freeze tests.
    sac: StellarAssetContract,
    admin: Address,
    funder: Address,
    provider: Address,
    attester: Address,
    fee_account: Address,
}

fn setup() -> Fixture {
    setup_with(FEE_BPS, ATTESTER_SHARE_BPS)
}

fn setup_with(fee_bps: u32, attester_share_bps: u32) -> Fixture {
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

    // Both instances exist before either is initialised, so the receipt book
    // can name the voucher contract as minter at once — the same order
    // scripts/deploy.sh uses. The two-step handover is covered on its own in
    // the receipt tests and in multisig_admin_can_act_on_all_three_contracts.
    let voucher_addr = env.register(VoucherEscrow, ());
    let voucher = VoucherEscrowClient::new(&env, &voucher_addr);

    let receipt_addr = env.register(ReceiptBook, ());
    let receipts = ReceiptBookClient::new(&env, &receipt_addr);
    receipts.initialize(&admin, &voucher_addr);

    voucher.initialize(
        &admin,
        &registry_addr,
        &receipt_addr,
        &token_addr,
        &DISPUTE_WINDOW,
        &fee_bps,
        &attester_share_bps,
        &fee_account,
    );

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
        sac,
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

/// Funds a voucher above the listed price, the way a mistyped amount does.
fn fund_overpaid(f: &Fixture, amount: i128) -> u64 {
    f.voucher.create_voucher(
        &f.funder,
        &patient(&f.env),
        &f.provider,
        &CONSULT,
        &amount,
        &(START_TIME + 30 * 24 * 60 * 60),
    )
}

/// Asserts the reported open escrow and the contract's real token balance are
/// both `expected`: the two drifting apart is the bug `escrow_summary` exists
/// to expose.
fn assert_escrow(f: &Fixture, expected: i128) {
    let (open_escrow, token_balance) = f.voucher.escrow_summary();
    assert_eq!(open_escrow, expected, "open_escrow is wrong");
    assert_eq!(token_balance, expected, "token balance is wrong");
}

// ----- escrow summary -----

#[test]
fn escrow_summary_tracks_the_settlement_lifecycle() {
    let f = setup();
    assert_escrow(&f, 0);

    let id = fund(&f);
    assert_escrow(&f, CONSULT_PRICE);

    f.voucher.claim(&f.provider, &id);
    assert_escrow(&f, CONSULT_PRICE);

    f.voucher.attest(&f.attester, &id);
    assert_escrow(&f, CONSULT_PRICE);

    advance(&f.env, DISPUTE_WINDOW + 1);
    f.voucher.settle(&id);
    assert_escrow(&f, 0);
}

#[test]
fn escrow_summary_tracks_a_refund() {
    let f = setup();
    let id = fund(&f);
    assert_escrow(&f, CONSULT_PRICE);

    advance(&f.env, 31 * 24 * 60 * 60);
    f.voucher.refund(&id);
    assert_escrow(&f, 0);
}

#[test]
fn escrow_summary_tracks_each_voucher_and_dispute_resolution() {
    let f = setup();
    let first = fund(&f);
    let second = fund(&f);
    assert_escrow(&f, 2 * CONSULT_PRICE);

    // Settle the first voucher; the second keeps its share in escrow.
    f.voucher.claim(&f.provider, &first);
    f.voucher.attest(&f.attester, &first);
    advance(&f.env, DISPUTE_WINDOW + 1);
    f.voucher.settle(&first);
    assert_escrow(&f, CONSULT_PRICE);

    // Dispute and refund the second; escrow is empty again.
    f.voucher.claim(&f.provider, &second);
    f.voucher.dispute(&f.funder, &second, &1u32);
    assert_escrow(&f, CONSULT_PRICE);
    f.voucher.resolve_dispute(&f.admin, &second, &true);
    assert_escrow(&f, 0);

    // The other outcome pays the provider and clears escrow as well.
    let third = fund(&f);
    assert_escrow(&f, CONSULT_PRICE);
    f.voucher.claim(&f.provider, &third);
    f.voucher.dispute(&f.funder, &third, &1u32);
    f.voucher.resolve_dispute(&f.admin, &third, &false);
    assert_escrow(&f, 0);
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

// ----- redirecting a funded voucher -----

fn register_redirect_provider(f: &Fixture) -> Address {
    let provider = Address::generate(&f.env);
    f.registry.register_provider(
        &provider,
        &String::from_str(&f.env, "New Clinic"),
        &String::from_str(&f.env, "NG"),
    );
    f.registry
        .set_provider_status(&f.admin, &provider, &ProviderStatus::Active);
    provider
}

#[test]
fn redirect_moves_a_funded_voucher_and_emits_event() {
    let f = setup();
    let id = fund(&f);
    let new_provider = register_redirect_provider(&f);
    f.registry.upsert_service(
        &new_provider,
        &CONSULT,
        &String::from_str(&f.env, "Outpatient consult"),
        &CONSULT_PRICE,
    );

    f.voucher.redirect(&f.funder, &id, &new_provider);

    let e = one_event(&f);
    assert_layout(
        &e,
        "voucher_redirected",
        &[ScVal::from(&f.funder), ScVal::from(&new_provider)],
        &["voucher_id", "previous_provider", "service_code"],
    );
    assert_eq!(field_u64(&e, "voucher_id"), id);
    assert_eq!(field_u32(&e, "service_code"), CONSULT);
    assert_eq!(field(&e, "previous_provider"), ScVal::from(&f.provider));

    let voucher = f.voucher.get_voucher(&id);
    assert_eq!(voucher.provider, new_provider);
    assert_eq!(voucher.status, VoucherStatus::Funded);
    assert_eq!(voucher.amount, CONSULT_PRICE);
}

#[test]
fn cannot_redirect_to_an_inactive_provider() {
    let f = setup();
    let id = fund(&f);
    let new_provider = register_redirect_provider(&f);
    f.registry
        .set_provider_status(&f.admin, &new_provider, &ProviderStatus::Suspended);

    let err = f
        .voucher
        .try_redirect(&f.funder, &id, &new_provider)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::ProviderNotActive);
}

#[test]
fn cannot_redirect_to_a_provider_without_the_service() {
    let f = setup();
    let id = fund(&f);
    let new_provider = register_redirect_provider(&f);

    let err = f
        .voucher
        .try_redirect(&f.funder, &id, &new_provider)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::ServiceNotOffered);
}

#[test]
fn cannot_redirect_when_new_provider_price_exceeds_voucher_amount() {
    let f = setup();
    let id = fund(&f);
    let new_provider = register_redirect_provider(&f);
    f.registry.upsert_service(
        &new_provider,
        &CONSULT,
        &String::from_str(&f.env, "Outpatient consult"),
        &(CONSULT_PRICE + 1),
    );

    let err = f
        .voucher
        .try_redirect(&f.funder, &id, &new_provider)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::AmountBelowPrice);
}

#[test]
fn cannot_redirect_a_claimed_voucher() {
    let f = setup();
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);
    let new_provider = register_redirect_provider(&f);
    f.registry.upsert_service(
        &new_provider,
        &CONSULT,
        &String::from_str(&f.env, "Outpatient consult"),
        &CONSULT_PRICE,
    );

    let err = f
        .voucher
        .try_redirect(&f.funder, &id, &new_provider)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::InvalidStatus);
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

// ----- reducing an overpayment -----
//
// A funder who paid more than the listed price can bring the escrowed
// amount back down to the price and keep the voucher. The difference goes
// back to the funder, and the voucher settles at the reduced amount.

#[test]
fn reduce_to_the_listed_price_succeeds() {
    let f = setup();
    let before = f.token.balance(&f.funder);
    let overpaid = CONSULT_PRICE * 2;
    let id = fund_overpaid(&f, overpaid);
    assert_eq!(f.token.balance(&f.funder), before - overpaid);

    f.voucher.reduce_amount(&f.funder, &id, &CONSULT_PRICE);

    // Exactly the difference comes back, and the escrow keeps the rest.
    assert_eq!(f.token.balance(&f.funder), before - CONSULT_PRICE);
    assert_eq!(f.token.balance(&f.voucher.address), CONSULT_PRICE);
    // The difference left escrow, so the running total follows it down.
    assert_escrow(&f, CONSULT_PRICE);

    let voucher = f.voucher.get_voucher(&id);
    assert_eq!(voucher.amount, CONSULT_PRICE);
    assert_eq!(
        voucher.status,
        VoucherStatus::Funded,
        "reducing an overpayment must not cost the funder the voucher"
    );
}

#[test]
fn reducing_twice_returns_the_difference_each_time() {
    let f = setup();
    let before = f.token.balance(&f.funder);
    let overpaid = CONSULT_PRICE * 3;
    let id = fund_overpaid(&f, overpaid);

    f.voucher
        .reduce_amount(&f.funder, &id, &(CONSULT_PRICE * 2));
    assert_eq!(f.token.balance(&f.funder), before - CONSULT_PRICE * 2);
    assert_eq!(f.voucher.get_voucher(&id).amount, CONSULT_PRICE * 2);

    f.voucher.reduce_amount(&f.funder, &id, &CONSULT_PRICE);
    // The funder recovers exactly the difference each time, and the escrow
    // keeps what is left.
    assert_eq!(f.token.balance(&f.funder), before - CONSULT_PRICE);
    assert_eq!(f.token.balance(&f.voucher.address), CONSULT_PRICE);
    assert_escrow(&f, CONSULT_PRICE);
    assert_eq!(f.voucher.get_voucher(&id).amount, CONSULT_PRICE);
}

#[test]
fn cannot_reduce_below_the_listed_price() {
    let f = setup();
    let overpaid = CONSULT_PRICE * 2;
    let id = fund_overpaid(&f, overpaid);

    let err = f
        .voucher
        .try_reduce_amount(&f.funder, &id, &(CONSULT_PRICE - 1))
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::AmountBelowPrice);

    // Nothing moved: the voucher still carries the original amount.
    assert_eq!(f.voucher.get_voucher(&id).amount, overpaid);
    assert_eq!(f.token.balance(&f.voucher.address), overpaid);
}

#[test]
fn cannot_reduce_after_the_voucher_is_claimed() {
    let f = setup();
    let id = fund_overpaid(&f, CONSULT_PRICE * 2);
    f.voucher.claim(&f.provider, &id);

    let err = f
        .voucher
        .try_reduce_amount(&f.funder, &id, &CONSULT_PRICE)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::InvalidStatus);
    assert_eq!(f.voucher.get_voucher(&id).amount, CONSULT_PRICE * 2);
}

#[test]
fn cannot_reduce_after_the_voucher_is_settled() {
    let f = setup();
    let id = fund_overpaid(&f, CONSULT_PRICE * 2);
    f.voucher.claim(&f.provider, &id);
    f.voucher.attest(&f.attester, &id);
    advance(&f.env, DISPUTE_WINDOW + 1);
    f.voucher.settle(&id);

    let err = f
        .voucher
        .try_reduce_amount(&f.funder, &id, &CONSULT_PRICE)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::InvalidStatus);
}

#[test]
fn only_the_funder_can_reduce_the_amount() {
    let f = setup();
    let id = fund_overpaid(&f, CONSULT_PRICE * 2);
    let stranger = Address::generate(&f.env);

    let err = f
        .voucher
        .try_reduce_amount(&stranger, &id, &CONSULT_PRICE)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::NotAuthorized);
    assert_eq!(f.voucher.get_voucher(&id).amount, CONSULT_PRICE * 2);
}

#[test]
fn cannot_reduce_to_the_same_or_a_higher_amount() {
    let f = setup();
    let overpaid = CONSULT_PRICE * 2;
    let id = fund_overpaid(&f, overpaid);

    let err = f
        .voucher
        .try_reduce_amount(&f.funder, &id, &overpaid)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::AmountNotReduced);

    let err = f
        .voucher
        .try_reduce_amount(&f.funder, &id, &(overpaid * 2))
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::AmountNotReduced);
    assert_eq!(f.voucher.get_voucher(&id).amount, overpaid);
}

#[test]
fn rejects_a_non_positive_new_amount() {
    let f = setup();
    let id = fund_overpaid(&f, CONSULT_PRICE * 2);

    let err = f
        .voucher
        .try_reduce_amount(&f.funder, &id, &0i128)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::InvalidAmount);

    let err = f
        .voucher
        .try_reduce_amount(&f.funder, &id, &(-1i128))
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::InvalidAmount);
    assert_eq!(f.voucher.get_voucher(&id).amount, CONSULT_PRICE * 2);
}

#[test]
fn cannot_reduce_when_the_service_is_no_longer_offered() {
    // No listed price means no floor to reduce towards. The funder can
    // still refund the voucher at expiry.
    let f = setup();
    let id = fund_overpaid(&f, CONSULT_PRICE * 2);
    f.registry.remove_service(&f.provider, &CONSULT);

    let err = f
        .voucher
        .try_reduce_amount(&f.funder, &id, &CONSULT_PRICE)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::ServiceNotOffered);
    assert_eq!(f.voucher.get_voucher(&id).amount, CONSULT_PRICE * 2);
}

#[test]
fn reduce_uses_the_price_at_call_time() {
    // A clinic that lowers its price frees the funder to reduce further.
    let f = setup();
    let overpaid = CONSULT_PRICE * 2;
    let id = fund_overpaid(&f, overpaid);
    let cheaper = CONSULT_PRICE / 2;

    f.registry.upsert_service(
        &f.provider,
        &CONSULT,
        &String::from_str(&f.env, "Outpatient consult"),
        &cheaper,
    );

    f.voucher.reduce_amount(&f.funder, &id, &cheaper);
    assert_eq!(f.voucher.get_voucher(&id).amount, cheaper);
}

#[test]
fn cannot_reduce_below_a_price_the_clinic_has_raised() {
    // The floor is the price at call time, not the price at funding time.
    let f = setup();
    let overpaid = CONSULT_PRICE * 2;
    let id = fund_overpaid(&f, overpaid);
    let effective_at = START_TIME + MIN_PRICE_NOTICE_SECS;
    f.registry
        .schedule_service_price(&f.provider, &CONSULT, &(CONSULT_PRICE * 3), &effective_at);
    f.env.ledger().with_mut(|li| li.timestamp = effective_at);

    let err = f
        .voucher
        .try_reduce_amount(&f.funder, &id, &CONSULT_PRICE)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::AmountBelowPrice);
    assert_eq!(f.voucher.get_voucher(&id).amount, overpaid);
}

#[test]
fn a_reduced_voucher_settles_at_the_reduced_amount() {
    let f = setup();
    let id = fund_overpaid(&f, CONSULT_PRICE * 2);
    f.voucher.reduce_amount(&f.funder, &id, &CONSULT_PRICE);

    f.voucher.claim(&f.provider, &id);
    f.voucher.attest(&f.attester, &id);
    advance(&f.env, DISPUTE_WINDOW + 1);
    f.voucher.settle(&id);

    // The clinic is paid on what is left in escrow, and the receipt says
    // the same thing.
    let expected_fee = CONSULT_PRICE * FEE_BPS as i128 / 10_000;
    let expected_attester_share = CONSULT_PRICE * ATTESTER_SHARE_BPS as i128 / 10_000;
    assert_eq!(f.token.balance(&f.provider), CONSULT_PRICE - expected_fee);
    assert_eq!(f.token.balance(&f.attester), expected_attester_share);
    assert_eq!(
        f.token.balance(&f.fee_account),
        expected_fee - expected_attester_share
    );
    assert_eq!(f.token.balance(&f.voucher.address), 0);
    assert_eq!(f.receipts.get_receipt(&id).amount, CONSULT_PRICE);
}

#[test]
fn amount_reduced_event_snapshot() {
    let f = setup();
    let overpaid = CONSULT_PRICE * 2;
    let id = fund_overpaid(&f, overpaid);

    f.voucher.reduce_amount(&f.funder, &id, &CONSULT_PRICE);

    let e = one_event(&f);
    assert_layout(
        &e,
        "voucher_amount_reduced",
        &[ScVal::from(&f.funder)],
        &[
            "voucher_id",
            "service_code",
            "previous_amount",
            "amount",
            "refunded",
        ],
    );
    assert_eq!(field_u64(&e, "voucher_id"), id);
    assert_eq!(field_u32(&e, "service_code"), CONSULT);
    assert_eq!(field_i128(&e, "previous_amount"), overpaid);
    assert_eq!(field_i128(&e, "amount"), CONSULT_PRICE);
    assert_eq!(field_i128(&e, "refunded"), overpaid - CONSULT_PRICE);
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
    let expected_attester_share = CONSULT_PRICE * ATTESTER_SHARE_BPS as i128 / 10_000;
    let expected_net = CONSULT_PRICE - expected_fee;

    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Settled);
    assert_eq!(f.token.balance(&f.provider), expected_net);
    assert_eq!(f.token.balance(&f.attester), expected_attester_share);
    assert_eq!(
        f.token.balance(&f.fee_account),
        expected_fee - expected_attester_share
    );
    assert_eq!(
        expected_net + expected_attester_share + (expected_fee - expected_attester_share),
        CONSULT_PRICE
    );
    assert_eq!(
        f.token.balance(&f.provider)
            + f.token.balance(&f.attester)
            + f.token.balance(&f.fee_account),
        CONSULT_PRICE
    );
    assert_eq!(f.token.balance(&f.voucher.address), 0);

    // Care receipt exists and carries no clinical data.
    let r = f.receipts.get_receipt(&id);
    assert_eq!(r.provider, f.provider);
    assert_eq!(r.service_code, CONSULT);
    assert_eq!(r.amount, CONSULT_PRICE);
    assert_eq!(f.receipts.count_for(&patient(&f.env)), 1);
}

#[test]
fn attestation_stores_the_attester_on_the_voucher() {
    let f = setup();
    let id = fund(&f);

    assert_eq!(f.voucher.get_voucher(&id).attester, None);
    f.voucher.claim(&f.provider, &id);
    f.voucher.attest(&f.attester, &id);

    let voucher = f.voucher.get_voucher(&id);
    assert_eq!(voucher.status, VoucherStatus::Attested);
    assert_eq!(voucher.attester, Some(f.attester.clone()));
}

/// Funds, claims, attests and settles a fresh voucher.
fn settle_fresh_voucher(f: &Fixture) -> u64 {
    let id = fund(f);
    f.voucher.claim(&f.provider, &id);
    f.voucher.attest(&f.attester, &id);
    advance(&f.env, DISPUTE_WINDOW + 1);
    f.voucher.settle(&id);
    id
}

#[test]
fn settlement_splits_the_fee_between_attester_and_fee_account() {
    // A quarter of the voucher amount is carved out of the 1% fee: 25 bps of
    // the amount to the attester, 75 bps to the fee account.
    let f = setup_with(FEE_BPS, 25);
    let id = settle_fresh_voucher(&f);

    let fee = CONSULT_PRICE * FEE_BPS as i128 / 10_000;
    let attester_share = CONSULT_PRICE * 25 / 10_000;
    let fee_account_share = fee - attester_share;
    let net = CONSULT_PRICE - fee;

    assert_eq!(f.token.balance(&f.provider), net);
    assert_eq!(f.token.balance(&f.attester), attester_share);
    assert_eq!(f.token.balance(&f.fee_account), fee_account_share);
    assert_eq!(net + attester_share + fee_account_share, CONSULT_PRICE);
    assert_eq!(f.token.balance(&f.voucher.address), 0);
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Settled);
}

#[test]
fn zero_attester_share_behaves_as_before() {
    let f = setup_with(FEE_BPS, 0);
    settle_fresh_voucher(&f);

    let fee = CONSULT_PRICE * FEE_BPS as i128 / 10_000;
    assert_eq!(f.token.balance(&f.provider), CONSULT_PRICE - fee);
    assert_eq!(f.token.balance(&f.attester), 0);
    assert_eq!(f.token.balance(&f.fee_account), fee);
    assert_eq!(f.token.balance(&f.voucher.address), 0);
}

#[test]
fn attester_share_equal_to_the_fee_pays_it_all_to_the_attester() {
    let f = setup_with(FEE_BPS, FEE_BPS);
    settle_fresh_voucher(&f);

    let fee = CONSULT_PRICE * FEE_BPS as i128 / 10_000;
    assert_eq!(f.token.balance(&f.provider), CONSULT_PRICE - fee);
    assert_eq!(f.token.balance(&f.attester), fee);
    assert_eq!(f.token.balance(&f.fee_account), 0);
    assert_eq!(f.token.balance(&f.voucher.address), 0);
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
    let attester_share = CONSULT_PRICE * ATTESTER_SHARE_BPS as i128 / 10_000;
    assert_eq!(f.token.balance(&f.attester), attester_share);
    assert_eq!(f.token.balance(&f.fee_account), fee - attester_share);
    assert_eq!(f.token.balance(&f.voucher.address), 0);
}

#[test]
fn quote_equals_settlement_for_every_amount() {
    // The app shows `quote` before the funder signs. If settlement paid
    // anything other than that, the funder would be misled at the moment
    // they commit money. Cover amounts below one fee unit (where integer
    // division rounds the fee down), amounts that land exactly on a unit,
    // ordinary prices, and the largest amount the fixture can fund.
    const CHEAP: u32 = 202;
    const CHEAP_PRICE: i128 = 1;
    // The funder starts with 1_000_000_000 units, so this is the maximum
    // amount a single voucher can carry in this fixture.
    const MAX_AMOUNT: i128 = 1_000_000_000;

    let amounts = [
        1i128,
        2,
        3,
        99,
        100,
        101,
        199,
        999,
        10_000,
        30_000_000,
        999_999_999,
        MAX_AMOUNT,
    ];

    for amount in amounts {
        let f = setup();
        f.registry.upsert_service(
            &f.provider,
            &CHEAP,
            &String::from_str(&f.env, "Cheap service"),
            &CHEAP_PRICE,
        );

        let (fee, net) = f.voucher.quote(&amount);
        let attester_share = amount * ATTESTER_SHARE_BPS as i128 / 10_000;
        assert_eq!(fee + net, amount, "quote does not add up for {amount}");
        assert_eq!(
            fee,
            amount * FEE_BPS as i128 / 10_000,
            "fee is not the documented basis-point cut for {amount}"
        );
        assert_eq!(
            net,
            amount - fee,
            "net is not amount minus fee for {amount}"
        );
        assert!(
            attester_share <= fee,
            "attester share exceeds the fee for {amount}"
        );

        let funder_before = f.token.balance(&f.funder);
        let id = f.voucher.create_voucher(
            &f.funder,
            &patient(&f.env),
            &f.provider,
            &CHEAP,
            &amount,
            &(START_TIME + 30 * 24 * 60 * 60),
        );
        f.voucher.claim(&f.provider, &id);
        f.voucher.attest(&f.attester, &id);
        advance(&f.env, DISPUTE_WINDOW + 1);
        f.voucher.settle(&id);

        assert_eq!(
            f.token.balance(&f.funder),
            funder_before - amount,
            "funder paid a different amount than quoted for {amount}"
        );
        assert_eq!(
            f.token.balance(&f.provider),
            net,
            "provider received a different net than quoted for {amount}"
        );
        assert_eq!(
            f.token.balance(&f.attester),
            attester_share,
            "attester received a different share than documented for {amount}"
        );
        assert_eq!(
            f.token.balance(&f.fee_account),
            fee - attester_share,
            "fee account received a different fee than quoted for {amount}"
        );
        assert_eq!(
            f.token.balance(&f.provider)
                + f.token.balance(&f.attester)
                + f.token.balance(&f.fee_account),
            amount,
            "settlement payouts do not sum to the voucher amount for {amount}"
        );
        assert_eq!(
            f.token.balance(&f.voucher.address),
            0,
            "escrow kept funds after settlement for {amount}"
        );
    }
}

#[test]
fn quote_rejects_an_amount_whose_fee_overflows() {
    let f = setup();
    let err = f.voucher.try_quote(&i128::MAX).err().unwrap().unwrap();
    assert_eq!(err, VoucherError::MathOverflow);
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
fn active_clinic_funded_voucher_still_requires_expiry() {
    // The new early-refund path is only for clinics that can no longer
    // serve the voucher. An active clinic keeps the original expiry rule.
    let f = setup();
    let id = fund(&f);
    let expires_at = f.voucher.get_voucher(&id).expires_at;

    assert_eq!(f.voucher.refundable_at(&id), expires_at);
    assert!(f.env.ledger().timestamp() < expires_at);

    let err = f.voucher.try_refund(&id).err().unwrap().unwrap();
    assert_eq!(err, VoucherError::NotYetExpired);
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Funded);
}

#[test]
fn funded_voucher_of_suspended_clinic_refunds_immediately() {
    // A suspended clinic cannot serve a funded voucher, so the funder must
    // not wait until expiry to get the money back.
    let f = setup();
    let before = f.token.balance(&f.funder);
    let id = fund(&f);
    let expires_at = f.voucher.get_voucher(&id).expires_at;

    f.registry
        .set_provider_status(&f.admin, &f.provider, &ProviderStatus::Suspended);
    assert!(f.env.ledger().timestamp() < expires_at);

    assert_eq!(
        f.voucher.refundable_at(&id),
        f.env.ledger().timestamp(),
        "a suspended clinic's funder should not wait for expiry"
    );

    f.voucher.refund(&id);

    // Full refund, no fee, and the provider gets nothing.
    assert_eq!(f.token.balance(&f.funder), before);
    assert_eq!(f.token.balance(&f.provider), 0);
    assert_eq!(f.token.balance(&f.fee_account), 0);
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Refunded);
}

#[test]
fn claimed_voucher_of_suspended_clinic_keeps_claim_grace() {
    // A claim means care may already have been given before the suspension.
    // Keep the normal path: an attester still has the claim grace to confirm
    // delivery, and only then can the funder take the money back.
    let f = setup();
    let before = f.token.balance(&f.funder);
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);

    f.registry
        .set_provider_status(&f.admin, &f.provider, &ProviderStatus::Suspended);

    let expires_at = f.voucher.get_voucher(&id).expires_at;
    let refundable_at = expires_at + crate::CLAIM_GRACE_SECS;

    // A day past expiry: suspension does not shorten the claim grace.
    f.env
        .ledger()
        .with_mut(|li| li.timestamp = expires_at + 24 * 60 * 60);
    assert_eq!(f.voucher.refundable_at(&id), refundable_at);

    let err = f.voucher.try_refund(&id).err().unwrap().unwrap();
    assert_eq!(err, VoucherError::NotYetExpired);
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Claimed);

    f.env.ledger().with_mut(|li| li.timestamp = refundable_at);
    f.voucher.refund(&id);

    assert_eq!(f.token.balance(&f.funder), before);
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Refunded);
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
    // No attestation happened, so there is no attester of record to pay and
    // the fee account keeps the whole fee.
    assert_eq!(f.token.balance(&f.attester), 0);
    assert_eq!(f.token.balance(&f.fee_account), expected_fee);
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

#[test]
fn split_zero_refunds_funder_in_full_without_a_receipt() {
    let f = setup();
    let before = f.token.balance(&f.funder);
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);
    f.voucher.dispute(&f.funder, &id, &1u32);

    f.voucher.resolve_split(&f.admin, &id, &0i128);

    assert_eq!(f.token.balance(&f.funder), before);
    assert_eq!(f.token.balance(&f.provider), 0);
    assert_eq!(f.token.balance(&f.fee_account), 0);
    assert_eq!(f.token.balance(&f.voucher.address), 0);
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Refunded);
    // Care did not happen, so the beneficiary has no care receipt.
    assert_eq!(f.receipts.count_for(&patient(&f.env)), 0);
}

#[test]
fn split_half_pays_clinic_fee_and_funder_exactly() {
    let f = setup();
    let before = f.token.balance(&f.funder);
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);
    f.voucher.dispute(&f.funder, &id, &1u32);

    let to_provider = CONSULT_PRICE / 2;
    let to_funder = CONSULT_PRICE - to_provider;
    let fee = to_provider * FEE_BPS as i128 / 10_000;
    let net = to_provider - fee;

    f.voucher.resolve_split(&f.admin, &id, &to_provider);

    assert_eq!(f.token.balance(&f.provider), net);
    assert_eq!(f.token.balance(&f.fee_account), fee);
    assert_eq!(f.token.balance(&f.funder), before - to_provider);
    assert_eq!(f.token.balance(&f.voucher.address), 0);
    assert_eq!(net + fee + to_funder, CONSULT_PRICE);
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Settled);

    let r = f.receipts.get_receipt(&id);
    assert_eq!(r.provider, f.provider);
    assert_eq!(r.service_code, CONSULT);
    assert_eq!(r.amount, to_provider);
    assert_eq!(f.receipts.count_for(&patient(&f.env)), 1);
}

#[test]
fn split_full_amount_matches_a_full_payout() {
    let f = setup();
    let before = f.token.balance(&f.funder);
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);
    f.voucher.dispute(&f.funder, &id, &1u32);

    let fee = CONSULT_PRICE * FEE_BPS as i128 / 10_000;
    let net = CONSULT_PRICE - fee;

    f.voucher.resolve_split(&f.admin, &id, &CONSULT_PRICE);

    assert_eq!(f.token.balance(&f.provider), net);
    assert_eq!(f.token.balance(&f.fee_account), fee);
    // The funder paid the voucher at funding time and gets nothing back.
    assert_eq!(f.token.balance(&f.funder), before - CONSULT_PRICE);
    assert_eq!(f.token.balance(&f.voucher.address), 0);
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Settled);

    let r = f.receipts.get_receipt(&id);
    assert_eq!(r.amount, CONSULT_PRICE);
    assert_eq!(f.receipts.count_for(&patient(&f.env)), 1);
}

#[test]
fn split_rejects_an_amount_outside_the_voucher() {
    let f = setup();
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);
    f.voucher.dispute(&f.funder, &id, &1u32);

    for bad in [-1i128, CONSULT_PRICE + 1] {
        let err = f
            .voucher
            .try_resolve_split(&f.admin, &id, &bad)
            .err()
            .unwrap()
            .unwrap();
        assert_eq!(err, VoucherError::InvalidAmount);
    }

    // Nothing moved and the dispute is still open.
    assert_eq!(f.token.balance(&f.provider), 0);
    assert_eq!(f.token.balance(&f.fee_account), 0);
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Disputed);
}

#[test]
fn non_admin_cannot_resolve_split() {
    let f = setup();
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);
    f.voucher.dispute(&f.funder, &id, &1u32);
    let impostor = Address::generate(&f.env);

    let err = f
        .voucher
        .try_resolve_split(&impostor, &id, &(CONSULT_PRICE / 2))
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::NotAuthorized);
}

#[test]
fn cannot_split_an_undisputed_voucher() {
    let f = setup();
    let id = fund(&f);

    let err = f
        .voucher
        .try_resolve_split(&f.admin, &id, &0i128)
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
        .try_initialize(
            &a,
            &a,
            &a,
            &a,
            &DISPUTE_WINDOW,
            &FEE_BPS,
            &ATTESTER_SHARE_BPS,
            &a,
        )
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
        .try_initialize(&a, &a, &a, &a, &DISPUTE_WINDOW, &1_001u32, &0u32, &a)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::InvalidFee);
}

#[test]
fn excessive_attester_share_rejected_at_init() {
    // The attester share is carved out of the fee, so a share above the fee
    // would pay out more than was charged.
    let env = Env::default();
    env.mock_all_auths();
    let a = Address::generate(&env);
    let addr = env.register(VoucherEscrow, ());
    let client = VoucherEscrowClient::new(&env, &addr);

    let err = client
        .try_initialize(
            &a,
            &a,
            &a,
            &a,
            &DISPUTE_WINDOW,
            &FEE_BPS,
            &(FEE_BPS + 1),
            &a,
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::InvalidAttesterShare);
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

    // Receipt book: hand the minter role over in two steps, then put it
    // back for the voucher.
    let new_minter = Address::generate(&f.env);
    f.receipts.propose_minter(&multisig, &new_minter);
    f.receipts.accept_minter(&new_minter);
    assert_eq!(f.receipts.get_minter(), new_minter);
    f.receipts.propose_minter(&multisig, &f.voucher.address);
    f.receipts.accept_minter(&f.voucher.address);

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
    // "what was paid" and "what landed". The attester's slice is part of the
    // fee, not an extra charge on the clinic.
    assert_eq!(field_i128(&e, "net") + field_i128(&e, "fee"), CONSULT_PRICE);
    assert_eq!(
        field_i128(&e, "attester_share"),
        CONSULT_PRICE * ATTESTER_SHARE_BPS as i128 / 10_000
    );
    assert_eq!(field_u32(&e, "service_code"), CONSULT);
    assert_eq!(field_i128(&e, "amount"), CONSULT_PRICE);
    assert_eq!(
        field_names(&e),
        [
            "amount",
            "attester_share",
            "fee",
            "net",
            "service_code",
            "voucher_id",
        ],
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
        function: "create_voucher_with_options",
        auth: signer("`funder`", funder_addr),
    },
    AuthCase {
        function: "reduce_amount",
        auth: signer("`funder`", funder_addr),
    },
    AuthCase {
        function: "redirect",
        auth: signer("`funder`", funder_addr),
    },
    AuthCase {
        function: "claim",
        auth: signer("`provider`", provider_addr),
    },
    AuthCase {
        function: "claim_with_proof",
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
        function: "resolve_split",
        auth: signer("`admin`", admin_addr),
    },
    AuthCase {
        function: "set_attestation_cap",
        auth: signer("`admin`", admin_addr),
    },
    AuthCase {
        function: "set_dispute_window_bounds",
        auth: signer("`admin`", admin_addr),
    },
    AuthCase {
        function: "set_max_escrow_per_provider",
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
        function: "get_dispute_window_bounds",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "get_voucher_options",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "get_max_escrow_per_provider",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "get_exposure",
        auth: Auth::Free("none (view)"),
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
        function: "escrow_summary",
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
                &ATTESTER_SHARE_BPS,
                &a,
            );
            return recorded_auths(&env);
        }
        "create_voucher" => {
            fund(&f.f);
        }
        "create_voucher_with_options" => {
            fund_with(&f.f, &VoucherOptions::default());
        }
        "reduce_amount" => {
            let id = fund_overpaid(&f.f, CONSULT_PRICE * 2);
            voucher.reduce_amount(&f.f.funder, &id, &CONSULT_PRICE);
        }
        "redirect" => {
            let id = fund(&f.f);
            let provider = Address::generate(env);
            f.f.registry.register_provider(
                &provider,
                &String::from_str(env, "New Clinic"),
                &String::from_str(env, "NG"),
            );
            f.f.registry
                .set_provider_status(&f.f.admin, &provider, &ProviderStatus::Active);
            f.f.registry.upsert_service(
                &provider,
                &CONSULT,
                &String::from_str(env, "Outpatient consult"),
                &CONSULT_PRICE,
            );
            voucher.redirect(&f.f.funder, &id, &provider);
        }
        "claim" => {
            let id = fund(&f.f);
            voucher.claim(&f.f.provider, &id);
        }
        "claim_with_proof" => {
            let secret = Bytes::from_slice(env, b"patient secret");
            let id = fund_with(
                &f.f,
                &VoucherOptions {
                    presence_commitment: Some(commit(env, &secret)),
                    dispute_window: None,
                },
            );
            voucher.claim_with_proof(&f.f.provider, &id, &secret);
        }
        "set_dispute_window_bounds" => {
            voucher.set_dispute_window_bounds(&f.f.admin, &60, &(60 * 60));
        }
        "get_dispute_window_bounds" => {
            voucher.get_dispute_window_bounds();
        }
        "get_voucher_options" => {
            let id = fund(&f.f);
            voucher.get_voucher_options(&id);
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
        "resolve_split" => {
            let id = fund(&f.f);
            voucher.claim(&f.f.provider, &id);
            voucher.dispute(&f.f.funder, &id, &1u32);
            voucher.resolve_split(&f.f.admin, &id, &(CONSULT_PRICE / 2));
        }
        "set_attestation_cap" => {
            voucher.set_attestation_cap(&f.f.admin, &10);
        }
        "set_max_escrow_per_provider" => {
            voucher.set_max_escrow_per_provider(&f.f.admin, &CONSULT_PRICE);
        }
        "get_max_escrow_per_provider" => {
            voucher.get_max_escrow_per_provider();
        }
        "get_exposure" => {
            voucher.get_exposure(&f.f.provider);
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
        "escrow_summary" => {
            voucher.escrow_summary();
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
    assert_eq!(
        f.voucher.get_config().attester_share_bps,
        ATTESTER_SHARE_BPS
    );
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
        &ATTESTER_SHARE_BPS,
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
        &[
            "voucher_id",
            "refunded_funder",
            "to_provider",
            "fee",
            "to_funder",
        ],
    );
    assert_eq!(field_u64(&e, "voucher_id"), id);
    assert!(field_bool(&e, "refunded_funder"));
    assert_eq!(field_i128(&e, "to_provider"), 0);
    assert_eq!(field_i128(&e, "fee"), 0);
    assert_eq!(field_i128(&e, "to_funder"), CONSULT_PRICE);
}

#[test]
fn split_dispute_resolved_event_snapshot() {
    let f = setup();
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);
    f.voucher.dispute(&f.funder, &id, &1u32);

    let to_provider = CONSULT_PRICE / 2;
    let fee = to_provider * FEE_BPS as i128 / 10_000;
    f.voucher.resolve_split(&f.admin, &id, &to_provider);

    // Splitting also mints a receipt, and that contract publishes its own
    // event in the same invocation; pick the voucher's event out by name.
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
        "splitting a dispute publishes exactly one dispute_resolved event",
    );
    let e = resolved[0].clone();
    assert_layout(
        &e,
        "dispute_resolved",
        &[ScVal::from(&f.admin)],
        &[
            "voucher_id",
            "refunded_funder",
            "to_provider",
            "fee",
            "to_funder",
        ],
    );
    assert_eq!(field_u64(&e, "voucher_id"), id);
    assert!(!field_bool(&e, "refunded_funder"));
    assert_eq!(field_i128(&e, "to_provider"), to_provider);
    assert_eq!(field_i128(&e, "fee"), fee);
    assert_eq!(field_i128(&e, "to_funder"), CONSULT_PRICE - to_provider);
    assert_eq!(
        field_i128(&e, "to_provider") + field_i128(&e, "to_funder"),
        CONSULT_PRICE
    );
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

// ----- voucher options: presence proof and per-voucher dispute window -----

fn fund_with(f: &Fixture, options: &VoucherOptions) -> u64 {
    f.voucher.create_voucher_with_options(
        &f.funder,
        &patient(&f.env),
        &f.provider,
        &CONSULT,
        &CONSULT_PRICE,
        &(START_TIME + 30 * 24 * 60 * 60),
        options,
    )
}

fn commit(env: &Env, secret: &Bytes) -> BytesN<32> {
    env.crypto().sha256(secret).to_bytes()
}

fn committed(f: &Fixture, secret: &[u8]) -> u64 {
    let secret = Bytes::from_slice(&f.env, secret);
    fund_with(
        f,
        &VoucherOptions {
            presence_commitment: Some(commit(&f.env, &secret)),
            dispute_window: None,
        },
    )
}

fn windowed(f: &Fixture, window: u64) -> u64 {
    fund_with(
        f,
        &VoucherOptions {
            presence_commitment: None,
            dispute_window: Some(window),
        },
    )
}

#[test]
fn committed_voucher_claims_with_the_right_secret() {
    let f = setup();
    let id = committed(&f, b"4471-ana");

    f.voucher
        .claim_with_proof(&f.provider, &id, &Bytes::from_slice(&f.env, b"4471-ana"));

    let v = f.voucher.get_voucher(&id);
    assert_eq!(v.status, VoucherStatus::Claimed);
    assert_eq!(v.claimed_at, START_TIME);
}

#[test]
fn committed_voucher_rejects_the_wrong_secret() {
    let f = setup();
    let id = committed(&f, b"4471-ana");

    assert_eq!(
        f.voucher
            .try_claim_with_proof(&f.provider, &id, &Bytes::from_slice(&f.env, b"4471-anb")),
        Err(Ok(VoucherError::InvalidPresenceProof))
    );
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Funded);
}

#[test]
fn committed_voucher_refuses_a_plain_claim() {
    let f = setup();
    let id = committed(&f, b"4471-ana");

    assert_eq!(
        f.voucher.try_claim(&f.provider, &id),
        Err(Ok(VoucherError::PresenceProofRequired))
    );
}

#[test]
fn proof_on_an_uncommitted_voucher_is_rejected() {
    let f = setup();
    let id = fund(&f);

    assert_eq!(
        f.voucher
            .try_claim_with_proof(&f.provider, &id, &Bytes::from_slice(&f.env, b"anything")),
        Err(Ok(VoucherError::NoPresenceCommitment))
    );
    // The plain path is unchanged for it.
    f.voucher.claim(&f.provider, &id);
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Claimed);
}

#[test]
fn proof_still_needs_the_named_provider() {
    let f = setup();
    let id = committed(&f, b"4471-ana");
    let other = Address::generate(&f.env);

    assert_eq!(
        f.voucher
            .try_claim_with_proof(&other, &id, &Bytes::from_slice(&f.env, b"4471-ana")),
        Err(Ok(VoucherError::NotAuthorized))
    );
}

#[test]
fn committed_voucher_settles_like_any_other() {
    let f = setup();
    let id = committed(&f, b"4471-ana");
    f.voucher
        .claim_with_proof(&f.provider, &id, &Bytes::from_slice(&f.env, b"4471-ana"));
    f.voucher.attest(&f.attester, &id);
    advance(&f.env, DISPUTE_WINDOW + 1);
    f.voucher.settle(&id);
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Settled);
}

#[test]
fn options_are_readable_and_plain_vouchers_have_none() {
    let f = setup();
    let plain = fund(&f);
    assert_eq!(
        f.voucher.get_voucher_options(&plain),
        VoucherOptions::default()
    );

    let secret = Bytes::from_slice(&f.env, b"s");
    let options = VoucherOptions {
        presence_commitment: Some(commit(&f.env, &secret)),
        dispute_window: Some(5 * 24 * 60 * 60),
    };
    let id = fund_with(&f, &options);
    assert_eq!(f.voucher.get_voucher_options(&id), options);

    assert_eq!(
        f.voucher.try_get_voucher_options(&99),
        Err(Ok(VoucherError::VoucherNotFound))
    );
}

#[test]
fn plain_voucher_writes_no_options_entry() {
    let f = setup();
    let id = fund(&f);
    let exists = f.env.as_contract(&f.voucher.address, || {
        f.env
            .storage()
            .persistent()
            .has(&crate::DataKey::Options(id))
    });
    assert!(
        !exists,
        "a voucher without options must not pay for a second entry"
    );
}

#[test]
fn default_dispute_window_is_unchanged() {
    let f = setup();
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);
    f.voucher.attest(&f.attester, &id);
    assert_eq!(
        f.voucher.get_voucher(&id).dispute_deadline,
        START_TIME + DISPUTE_WINDOW
    );
}

#[test]
fn short_and_long_windows_set_their_own_deadline() {
    let f = setup();
    let day = 24 * 60 * 60;

    for window in [day, 10 * day, 30 * day] {
        let id = windowed(&f, window);
        f.voucher.claim(&f.provider, &id);
        let now = f.env.ledger().timestamp();
        f.voucher.attest(&f.attester, &id);
        assert_eq!(f.voucher.get_voucher(&id).dispute_deadline, now + window);
    }
}

#[test]
fn short_window_settles_early_and_long_window_waits() {
    let f = setup();
    let day = 24 * 60 * 60;
    let short = windowed(&f, day);
    let long = windowed(&f, 10 * day);
    for id in [short, long] {
        f.voucher.claim(&f.provider, &id);
        f.voucher.attest(&f.attester, &id);
    }

    advance(&f.env, day);
    f.voucher.settle(&short);
    assert_eq!(
        f.voucher.try_settle(&long),
        Err(Ok(VoucherError::DisputeWindowOpen))
    );
    // The funder of the long voucher can still dispute.
    f.voucher.dispute(&f.funder, &long, &1u32);
}

#[test]
fn dispute_window_outside_the_bounds_is_rejected() {
    let f = setup();
    let (min, max) = DEFAULT_DISPUTE_WINDOW_BOUNDS;
    let try_window = |window: u64| {
        f.voucher.try_create_voucher_with_options(
            &f.funder,
            &patient(&f.env),
            &f.provider,
            &CONSULT,
            &CONSULT_PRICE,
            &(START_TIME + 30 * 24 * 60 * 60),
            &VoucherOptions {
                presence_commitment: None,
                dispute_window: Some(window),
            },
        )
    };

    assert_eq!(
        try_window(min - 1),
        Err(Ok(VoucherError::InvalidDisputeWindow))
    );
    assert_eq!(
        try_window(max + 1),
        Err(Ok(VoucherError::InvalidDisputeWindow))
    );
    assert_eq!(try_window(0), Err(Ok(VoucherError::InvalidDisputeWindow)));
    assert!(try_window(min).is_ok());
    assert!(try_window(max).is_ok());
}

#[test]
fn admin_moves_the_dispute_window_bounds() {
    let f = setup();
    assert_eq!(
        f.voucher.get_dispute_window_bounds(),
        DEFAULT_DISPUTE_WINDOW_BOUNDS
    );

    f.voucher
        .set_dispute_window_bounds(&f.admin, &60, &(2 * 60 * 60));
    assert_eq!(f.voucher.get_dispute_window_bounds(), (60, 2 * 60 * 60));

    let id = windowed(&f, 60);
    f.voucher.claim(&f.provider, &id);
    f.voucher.attest(&f.attester, &id);
    assert_eq!(f.voucher.get_voucher(&id).dispute_deadline, START_TIME + 60);
}

#[test]
fn dispute_window_bounds_are_checked() {
    let f = setup();
    let stranger = Address::generate(&f.env);
    assert_eq!(
        f.voucher
            .try_set_dispute_window_bounds(&stranger, &60, &120),
        Err(Ok(VoucherError::NotAuthorized))
    );
    assert_eq!(
        f.voucher.try_set_dispute_window_bounds(&f.admin, &0, &120),
        Err(Ok(VoucherError::InvalidDisputeWindow))
    );
    assert_eq!(
        f.voucher
            .try_set_dispute_window_bounds(&f.admin, &121, &120),
        Err(Ok(VoucherError::InvalidDisputeWindow))
    );
}

#[test]
fn dispute_window_bounds_changed_event_snapshot() {
    let f = setup();

    f.voucher
        .set_dispute_window_bounds(&f.admin, &3600, &86_400);

    let e = one_event(&f);
    assert_layout(
        &e,
        "dispute_window_bounds_changed",
        &[ScVal::from(&f.admin)],
        &["min", "max"],
    );
    assert_eq!(field_u64(&e, "min"), 3600);
    assert_eq!(field_u64(&e, "max"), 86_400);
}

// ----- storage footprint -----

/// The voucher entry is stored positionally (an XDR vec) without its id
/// instead of as a named struct (an XDR map carrying every field name). This
/// pins that the stored form is smaller and prints both sizes, which are the
/// numbers rent is charged on.
#[test]
fn stored_voucher_is_smaller_than_the_named_layout() {
    use soroban_sdk::xdr::ToXdr;

    let f = setup();
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);
    f.voucher.attest(&f.attester, &id);
    let voucher = f.voucher.get_voucher(&id);

    let named = voucher.clone().to_xdr(&f.env).len();
    let stored = f.env.as_contract(&f.voucher.address, || {
        let stored: crate::storage::StoredVoucher = f
            .env
            .storage()
            .persistent()
            .get(&crate::DataKey::Voucher(id))
            .expect("the voucher is stored");
        stored.to_xdr(&f.env).len()
    });

    std::println!(
        "voucher entry value: {named} bytes as a named struct, {stored} bytes as stored \
         ({}% smaller)",
        (named - stored) * 100 / named
    );
    assert!(
        stored < named,
        "the stored voucher ({stored} B) must be smaller than the named layout ({named} B)"
    );
}

/// Every field survives the positional layout: what is read back is what was
/// written, field for field, through a full lifecycle.
#[test]
fn stored_voucher_round_trips_every_field() {
    let f = setup();
    let id = fund(&f);
    advance(&f.env, 10);
    f.voucher.claim(&f.provider, &id);
    advance(&f.env, 10);
    f.voucher.attest(&f.attester, &id);

    let v = f.voucher.get_voucher(&id);
    assert_eq!(v.id, id);
    assert_eq!(v.funder, f.funder);
    assert_eq!(v.beneficiary_ref, patient(&f.env));
    assert_eq!(v.provider, f.provider);
    assert_eq!(v.service_code, CONSULT);
    assert_eq!(v.amount, CONSULT_PRICE);
    assert_eq!(v.status, VoucherStatus::Attested);
    assert_eq!(v.created_at, START_TIME);
    assert_eq!(v.expires_at, START_TIME + 30 * 24 * 60 * 60);
    assert_eq!(v.claimed_at, START_TIME + 10);
    assert_eq!(v.attested_at, START_TIME + 20);
    assert_eq!(v.dispute_deadline, START_TIME + 20 + DISPUTE_WINDOW);
}

// ----- clawback and frozen settlement token -----
//
// USDC's issuer can claw funds back from any balance and freeze
// (deauthorize) one. Escrow is one shared balance, so these tests check that
// every payout path fails with a named error and reverts cleanly instead of
// panicking inside the token or paying out part of a voucher.

/// The issuer-side client of the settlement token, with clawback and
/// revocation enabled. Call before funding: a balance records whether it can
/// be clawed back when it is created.
fn issuer_with_clawback(f: &Fixture) -> token::StellarAssetClient<'static> {
    let issuer = f.sac.issuer();
    issuer.set_flag(IssuerFlags::RevocableFlag);
    issuer.set_flag(IssuerFlags::ClawbackEnabledFlag);
    token::StellarAssetClient::new(&f.env, &f.sac.address())
}

fn attested(f: &Fixture) -> u64 {
    let id = claimed(f);
    f.voucher.attest(&f.attester, &id);
    advance(&f.env, DISPUTE_WINDOW + 1);
    id
}

#[test]
fn clawback_then_settle_returns_escrow_shortfall() {
    let f = setup();
    let issuer = issuer_with_clawback(&f);
    let id = attested(&f);
    issuer.clawback(&f.voucher.address, &1);

    let err = f.voucher.try_settle(&id).err().unwrap().unwrap();
    assert_eq!(err, VoucherError::EscrowShortfall);
    // Nothing moved and nothing was recorded.
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Attested);
    assert_eq!(f.token.balance(&f.provider), 0);
    assert_eq!(f.receipts.count_for(&patient(&f.env)), 0);
}

#[test]
fn clawback_then_refund_returns_escrow_shortfall() {
    let f = setup();
    let issuer = issuer_with_clawback(&f);
    let id = fund(&f);
    issuer.clawback(&f.voucher.address, &1);
    advance(&f.env, 31 * 24 * 60 * 60);

    let err = f.voucher.try_refund(&id).err().unwrap().unwrap();
    assert_eq!(err, VoucherError::EscrowShortfall);
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Funded);
}

#[test]
fn clawback_then_resolving_a_dispute_returns_escrow_shortfall() {
    let f = setup();
    let issuer = issuer_with_clawback(&f);
    let id = claimed(&f);
    f.voucher.dispute(&f.funder, &id, &1u32);
    issuer.clawback(&f.voucher.address, &1);

    for refund_funder in [true, false] {
        let err = f
            .voucher
            .try_resolve_dispute(&f.admin, &id, &refund_funder)
            .err()
            .unwrap()
            .unwrap();
        assert_eq!(err, VoucherError::EscrowShortfall);
    }
    let err = f
        .voucher
        .try_resolve_split(&f.admin, &id, &(CONSULT_PRICE / 2))
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::EscrowShortfall);
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Disputed);
}

#[test]
fn clawback_then_reducing_the_amount_returns_escrow_shortfall() {
    let f = setup();
    let issuer = issuer_with_clawback(&f);
    let id = fund_overpaid(&f, CONSULT_PRICE * 2);
    issuer.clawback(&f.voucher.address, &1);

    let err = f
        .voucher
        .try_reduce_amount(&f.funder, &id, &CONSULT_PRICE)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::EscrowShortfall);
    assert_eq!(f.voucher.get_voucher(&id).amount, CONSULT_PRICE * 2);
}

#[test]
fn partial_clawback_falls_on_the_last_voucher_out() {
    let f = setup();
    let issuer = issuer_with_clawback(&f);
    let first = claimed(&f);
    let second = claimed(&f);
    f.voucher.attest(&f.attester, &first);
    f.voucher.attest(&f.attester, &second);
    advance(&f.env, DISPUTE_WINDOW + 1);
    issuer.clawback(&f.voucher.address, &1);

    // The shared balance still covers one voucher in full...
    f.voucher.settle(&first);
    assert_eq!(f.voucher.get_voucher(&first).status, VoucherStatus::Settled);
    // ...and not the second, which keeps its status until the balance is restored.
    let err = f.voucher.try_settle(&second).err().unwrap().unwrap();
    assert_eq!(err, VoucherError::EscrowShortfall);
    assert_eq!(
        f.voucher.get_voucher(&second).status,
        VoucherStatus::Attested
    );

    issuer.mint(&f.voucher.address, &1);
    f.voucher.settle(&second);
    assert_eq!(
        f.voucher.get_voucher(&second).status,
        VoucherStatus::Settled
    );
}

#[test]
fn frozen_escrow_returns_transfer_failed_until_unfrozen() {
    let f = setup();
    let issuer = issuer_with_clawback(&f);
    let id = attested(&f);
    issuer.set_authorized(&f.voucher.address, &false);

    let err = f.voucher.try_settle(&id).err().unwrap().unwrap();
    assert_eq!(err, VoucherError::TokenTransferFailed);
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Attested);
    assert_eq!(f.token.balance(&f.voucher.address), CONSULT_PRICE);

    issuer.set_authorized(&f.voucher.address, &true);
    f.voucher.settle(&id);
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Settled);
}

#[test]
fn frozen_provider_returns_transfer_failed() {
    let f = setup();
    let issuer = issuer_with_clawback(&f);
    let id = attested(&f);
    issuer.set_authorized(&f.provider, &false);

    let err = f.voucher.try_settle(&id).err().unwrap().unwrap();
    assert_eq!(err, VoucherError::TokenTransferFailed);
    // The whole call reverted: nobody was paid, not even the fee.
    assert_eq!(f.token.balance(&f.fee_account), 0);
    assert_eq!(f.token.balance(&f.attester), 0);
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Attested);
}

#[test]
fn frozen_funder_returns_transfer_failed_on_refund() {
    let f = setup();
    let issuer = issuer_with_clawback(&f);
    let id = fund(&f);
    issuer.set_authorized(&f.funder, &false);
    advance(&f.env, 31 * 24 * 60 * 60);

    let err = f.voucher.try_refund(&id).err().unwrap().unwrap();
    assert_eq!(err, VoucherError::TokenTransferFailed);
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Funded);
}

// ----- restored from main (dropped by an earlier merge of this branch) -----

#[test]
fn not_initialized_errors() {
    let env = Env::default();
    env.mock_all_auths();
    let a = Address::generate(&env);
    let addr = env.register(VoucherEscrow, ());
    let client = VoucherEscrowClient::new(&env, &addr);

    // Calls that read the config before initialize should return NotInitialized.
    let err = client.try_get_config().err().unwrap().unwrap();
    assert_eq!(err, VoucherError::NotInitialized);
    // get_voucher returns VoucherNotFound because the voucher doesn't exist,
    // not because the contract isn't initialized.
    let err = client.try_get_voucher(&1u64).err().unwrap().unwrap();
    assert_eq!(err, VoucherError::VoucherNotFound);
    // create_voucher requires config, so it returns NotInitialized.
    let err = client
        .try_create_voucher(
            &a,
            &BytesN::from_array(&env, &[1u8; 32]),
            &a,
            &1u32,
            &100i128,
            &(env.ledger().timestamp() + 1000),
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::NotInitialized);
}

#[test]
fn math_overflow_in_fee_calculation() {
    // If fee_bps and amount are both large, multiplication can overflow.
    // This is exercised by the internal fee_for function. We test the
    // internal path by using a large amount and fee_bps.
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = START_TIME);

    let admin = Address::generate(&env);
    let funder = Address::generate(&env);
    let provider = Address::generate(&env);
    let attester = Address::generate(&env);
    let fee_account = Address::generate(&env);

    let issuer = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(issuer);
    let token_addr = sac.address();
    let _token_client = token::Client::new(&env, &token_addr);
    let token_admin = token::StellarAssetClient::new(&env, &token_addr);
    token_admin.mint(&funder, &i128::MAX);

    let registry_addr = env.register(Registry, ());
    let registry = RegistryClient::new(&env, &registry_addr);
    registry.initialize(&admin);
    registry.register_provider(
        &provider,
        &String::from_str(&env, "Clinic"),
        &String::from_str(&env, "NG"),
    );
    registry.set_provider_status(&admin, &provider, &ProviderStatus::Active);
    registry.upsert_service(
        &provider,
        &CONSULT,
        &String::from_str(&env, "Consult"),
        &CONSULT_PRICE,
    );
    registry.add_attester(&admin, &attester);

    let voucher_addr = env.register(VoucherEscrow, ());
    let voucher = VoucherEscrowClient::new(&env, &voucher_addr);

    let receipt_addr = env.register(ReceiptBook, ());
    let receipts = ReceiptBookClient::new(&env, &receipt_addr);
    receipts.initialize(&admin, &voucher_addr);
    // Use a fee_bps that makes the multiplication overflow i128.
    // fee_bps = MAX_FEE_BPS = 1000 (10%), amount = i128::MAX
    // i128::MAX * 1000 > i128::MAX
    voucher.initialize(
        &admin,
        &registry_addr,
        &receipt_addr,
        &token_addr,
        &DISPUTE_WINDOW,
        &1_000u32,
        &0u32,
        &fee_account,
    );

    let id = voucher.create_voucher(
        &funder,
        &patient(&env),
        &provider,
        &CONSULT,
        &i128::MAX,
        &(START_TIME + 30 * 24 * 60 * 60),
    );
    // settle() calls fee_for internally which can overflow
    voucher.claim(&provider, &id);
    voucher.attest(&attester, &id);
    advance(&env, DISPUTE_WINDOW + 1);
    let err = voucher.try_settle(&id).err().unwrap().unwrap();
    assert_eq!(err, VoucherError::MathOverflow);
}

// ----- clinic capacity and per-clinic escrow cap -----

fn settle_one(f: &Fixture, id: u64) {
    f.voucher.claim(&f.provider, &id);
    f.voucher.attest(&f.attester, &id);
    advance(&f.env, DISPUTE_WINDOW + 1);
    f.voucher.settle(&id);
}

fn try_fund(f: &Fixture, amount: i128) -> Result<u64, VoucherError> {
    match f.voucher.try_create_voucher(
        &f.funder,
        &patient(&f.env),
        &f.provider,
        &CONSULT,
        &amount,
        &(f.env.ledger().timestamp() + 30 * 24 * 60 * 60),
    ) {
        Ok(Ok(id)) => Ok(id),
        Err(Ok(e)) => Err(e),
        other => panic!("unexpected result {other:?}"),
    }
}

#[test]
fn exposure_tracks_open_vouchers() {
    let f = setup();
    assert_eq!(f.voucher.get_exposure(&f.provider), Exposure::default());

    let a = fund(&f);
    fund(&f);
    assert_eq!(
        f.voucher.get_exposure(&f.provider),
        Exposure {
            vouchers: 2,
            value: 2 * CONSULT_PRICE
        }
    );

    settle_one(&f, a);
    assert_eq!(
        f.voucher.get_exposure(&f.provider),
        Exposure {
            vouchers: 1,
            value: CONSULT_PRICE
        }
    );
}

#[test]
fn capacity_rejects_vouchers_until_one_settles() {
    let f = setup();
    f.registry.set_capacity(&f.provider, &2);

    let first = fund(&f);
    fund(&f);
    assert_eq!(
        try_fund(&f, CONSULT_PRICE),
        Err(VoucherError::ProviderAtCapacity)
    );

    settle_one(&f, first);
    assert!(try_fund(&f, CONSULT_PRICE).is_ok(), "settling frees a slot");
    assert_eq!(
        try_fund(&f, CONSULT_PRICE),
        Err(VoucherError::ProviderAtCapacity)
    );
}

#[test]
fn capacity_frees_a_slot_on_refund() {
    let f = setup();
    f.registry.set_capacity(&f.provider, &1);

    let id = fund(&f);
    assert_eq!(
        try_fund(&f, CONSULT_PRICE),
        Err(VoucherError::ProviderAtCapacity)
    );

    advance(&f.env, 31 * 24 * 60 * 60);
    f.voucher.refund(&id);
    assert_eq!(f.voucher.get_exposure(&f.provider), Exposure::default());
    assert!(
        try_fund(&f, CONSULT_PRICE).is_ok(),
        "refunding frees a slot"
    );
}

#[test]
fn capacity_frees_a_slot_when_a_dispute_resolves() {
    let f = setup();
    f.registry.set_capacity(&f.provider, &1);

    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);
    f.voucher.dispute(&f.funder, &id, &1u32);
    assert_eq!(
        try_fund(&f, CONSULT_PRICE),
        Err(VoucherError::ProviderAtCapacity)
    );

    f.voucher.resolve_dispute(&f.admin, &id, &true);
    assert!(try_fund(&f, CONSULT_PRICE).is_ok());
}

#[test]
fn zero_capacity_is_unlimited() {
    let f = setup();
    f.registry.set_capacity(&f.provider, &0);
    for _ in 0..5 {
        fund(&f);
    }
    assert_eq!(f.voucher.get_exposure(&f.provider).vouchers, 5);
}

#[test]
fn escrow_cap_allows_exactly_the_cap() {
    let f = setup();
    f.voucher
        .set_max_escrow_per_provider(&f.admin, &(2 * CONSULT_PRICE));
    assert_eq!(f.voucher.get_max_escrow_per_provider(), 2 * CONSULT_PRICE);

    fund(&f);
    fund(&f);
    // At the cap: one stroop more is refused.
    assert_eq!(
        try_fund(&f, CONSULT_PRICE),
        Err(VoucherError::EscrowCapReached)
    );
    assert_eq!(f.voucher.get_exposure(&f.provider).value, 2 * CONSULT_PRICE);
}

#[test]
fn escrow_cap_counts_the_new_amount() {
    let f = setup();
    f.voucher
        .set_max_escrow_per_provider(&f.admin, &(CONSULT_PRICE + 5));

    fund(&f);
    // 5 stroops of room is below the listed price, so a voucher cannot fit.
    assert_eq!(
        try_fund(&f, CONSULT_PRICE),
        Err(VoucherError::EscrowCapReached)
    );
}

#[test]
fn escrow_cap_releases_value_on_settle_and_refund() {
    let f = setup();
    f.voucher
        .set_max_escrow_per_provider(&f.admin, &CONSULT_PRICE);

    let id = fund(&f);
    assert_eq!(
        try_fund(&f, CONSULT_PRICE),
        Err(VoucherError::EscrowCapReached)
    );
    settle_one(&f, id);

    let id = try_fund(&f, CONSULT_PRICE).expect("settle released the value");
    assert_eq!(
        try_fund(&f, CONSULT_PRICE),
        Err(VoucherError::EscrowCapReached)
    );
    advance(&f.env, 31 * 24 * 60 * 60);
    f.voucher.refund(&id);

    assert!(
        try_fund(&f, CONSULT_PRICE).is_ok(),
        "refund released the value"
    );
}

#[test]
fn escrow_cap_is_admin_only_and_not_negative() {
    let f = setup();
    let stranger = Address::generate(&f.env);
    assert_eq!(
        f.voucher.try_set_max_escrow_per_provider(&stranger, &1),
        Err(Ok(VoucherError::NotAuthorized))
    );
    assert_eq!(
        f.voucher.try_set_max_escrow_per_provider(&f.admin, &-1),
        Err(Ok(VoucherError::InvalidEscrowCap))
    );
    assert_eq!(f.voucher.get_max_escrow_per_provider(), 0);
}

#[test]
fn lowering_the_escrow_cap_does_not_trap_funded_vouchers() {
    let f = setup();
    let id = fund(&f);
    fund(&f);
    f.voucher.set_max_escrow_per_provider(&f.admin, &1);

    assert_eq!(
        try_fund(&f, CONSULT_PRICE),
        Err(VoucherError::EscrowCapReached)
    );
    settle_one(&f, id);
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Settled);
}

#[test]
fn escrow_cap_changed_event_snapshot() {
    let f = setup();

    f.voucher
        .set_max_escrow_per_provider(&f.admin, &(5 * CONSULT_PRICE));

    let e = one_event(&f);
    assert_layout(&e, "escrow_cap_changed", &[ScVal::from(&f.admin)], &["max"]);
    assert_eq!(field_i128(&e, "max"), 5 * CONSULT_PRICE);
}

#[test]
fn split_resolution_releases_open_escrow_and_exposure() {
    let f = setup();
    let id = claimed(&f);
    f.voucher.dispute(&f.funder, &id, &1u32);

    f.voucher.resolve_split(&f.admin, &id, &(CONSULT_PRICE / 2));

    let (open_escrow, token_balance) = f.voucher.escrow_summary();
    assert_eq!(
        open_escrow, 0,
        "the split voucher still counts as open escrow"
    );
    assert_eq!(token_balance, 0);
    assert_eq!(f.voucher.get_exposure(&f.provider), Exposure::default());
}

#[test]
fn reducing_a_voucher_lowers_the_clinics_exposure() {
    let f = setup();
    fund_overpaid(&f, CONSULT_PRICE * 2);
    let id = fund_overpaid(&f, CONSULT_PRICE * 2);

    f.voucher.reduce_amount(&f.funder, &id, &CONSULT_PRICE);

    assert_eq!(
        f.voucher.get_exposure(&f.provider),
        Exposure {
            vouchers: 2,
            value: 3 * CONSULT_PRICE
        }
    );
    let (open_escrow, _) = f.voucher.escrow_summary();
    assert_eq!(open_escrow, 3 * CONSULT_PRICE);
}
