#![cfg(test)]
extern crate std;

use soroban_sdk::{
    testutils::{
        storage::{Instance as _, Persistent as _},
        Address as _, Events as _, Ledger as _,
    },
    token, vec, Address, BytesN, Env, String, Vec,
};

use sci_receipt::{ReceiptBook, ReceiptBookClient};
use sci_registry::{ProviderStatus, Registry, RegistryClient};
use sci_voucher::{VoucherEscrow, VoucherEscrowClient};

use crate::{Rules, SponsorDesk, SponsorDeskClient, SponsorError, MAX_MATCH_BPS, MAX_OPEN_MATCHES};

const START_TIME: u64 = 1_700_000_000;
const CONSULT: u32 = 101;
const CONSULT_PRICE: i128 = 3_000_000; // 0.30 USDC at 7 decimals
/// A cheap service, so a ratio small enough to round down to nothing can
/// be tried.
const CHEAP: u32 = 102;
const CHEAP_PRICE: i128 = 100;
/// One-for-one: the sponsor adds as much as the funder put in.
const ONE_FOR_ONE: u32 = 10_000;
const DAY: u64 = 24 * 60 * 60;
/// The budget the matching tests deposit.
const BUDGET: i128 = 10_000_000;
/// A cap above the price of the care, so the cap never makes a match
/// impossible on its own.
const CAP: i128 = 10_000_000;

struct Fixture {
    env: Env,
    desk_addr: Address,
    registry_addr: Address,
    voucher_addr: Address,
    token_addr: Address,
    desk: SponsorDeskClient<'static>,
    voucher: VoucherEscrowClient<'static>,
    registry: RegistryClient<'static>,
    token: token::Client<'static>,
    admin: Address,
    sponsor: Address,
    funder: Address,
    provider: Address,
    attester: Address,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = START_TIME);

    let admin = Address::generate(&env);
    let sponsor = Address::generate(&env);
    let funder = Address::generate(&env);
    let provider = Address::generate(&env);
    let attester = Address::generate(&env);

    let issuer = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(issuer);
    let token_addr = sac.address();
    let token_client = token::Client::new(&env, &token_addr);
    token::StellarAssetClient::new(&env, &token_addr).mint(&funder, &1_000_000_000);
    token::StellarAssetClient::new(&env, &token_addr).mint(&sponsor, &1_000_000_000);

    let registry_addr = env.register(Registry, ());
    let registry = RegistryClient::new(&env, &registry_addr);
    registry.initialize(&admin);
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
    registry.upsert_service(
        &provider,
        &CHEAP,
        &String::from_str(&env, "Follow-up note"),
        &CHEAP_PRICE,
    );
    registry.add_attester(&admin, &attester);

    // Both instances exist before either is initialised, so the receipt book
    // can name the voucher contract as minter at once — the order
    // scripts/deploy.sh uses.
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
        &(72 * 60 * 60),
        &100,
        &50,
        &admin,
    );

    let desk_addr = env.register(SponsorDesk, ());
    let desk = SponsorDeskClient::new(&env, &desk_addr);
    desk.initialize(&admin, &registry_addr, &voucher_addr, &token_addr);

    Fixture {
        env,
        desk_addr,
        registry_addr,
        voucher_addr,
        token_addr,
        desk,
        voucher,
        registry,
        token: token_client,
        admin,
        sponsor,
        funder,
        provider,
        attester,
    }
}

fn patient(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[9u8; 32])
}

/// The rules a sponsor starts with: one-for-one, capped, no country or
/// service restriction.
fn rules(env: &Env, match_bps: u32, cap: i128) -> Rules {
    Rules {
        match_bps,
        cap_per_voucher: cap,
        min_voucher_amount: 0,
        min_voucher_age_secs: 0,
        eligible_countries: Vec::new(env),
        eligible_service_codes: Vec::new(env),
    }
}

fn advance(env: &Env, secs: u64) {
    env.ledger().with_mut(|li| li.timestamp += secs);
}

/// Funds an ordinary voucher for the consult, paid by the funder.
fn fund(f: &Fixture) -> u64 {
    f.voucher.create_voucher(
        &f.funder,
        &patient(&f.env),
        &f.provider,
        &CONSULT,
        &CONSULT_PRICE,
        &(START_TIME + 30 * DAY),
    )
}

// ----- deposits -----

#[test]
fn deposit_moves_money_into_the_desk() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &1_000_000);

    assert_eq!(f.desk.available_budget(&f.sponsor), 1_000_000);
    assert_eq!(f.token.balance(&f.sponsor), 999_000_000);
    // The money is in the desk, and the desk says so.
    let (_, balance) = f.desk.budget_summary();
    assert_eq!(balance, 1_000_000);
}

#[test]
fn deposit_rejects_a_non_positive_amount() {
    let f = setup();
    assert_eq!(
        f.desk.try_deposit(&f.sponsor, &0),
        Err(Ok(SponsorError::InvalidAmount))
    );
    assert_eq!(
        f.desk.try_deposit(&f.sponsor, &-1),
        Err(Ok(SponsorError::InvalidAmount))
    );
}

#[test]
fn deposit_needs_the_sponsors_signature() {
    let f = setup();
    f.env.mock_auths(&[]);
    let err = f.desk.try_deposit(&f.sponsor, &1_000_000);
    assert!(err.is_err(), "an unsigned deposit must not go through");
}

// ----- rules -----

#[test]
fn rules_are_stored_and_read_back() {
    let f = setup();
    let rules = Rules {
        match_bps: 5_000,
        cap_per_voucher: 500_000,
        min_voucher_amount: 1_000,
        min_voucher_age_secs: 60,
        eligible_countries: vec![&f.env, String::from_str(&f.env, "NG")],
        eligible_service_codes: vec![&f.env, CONSULT],
    };
    f.desk.set_rules(&f.sponsor, &rules);

    assert_eq!(f.desk.get_rules(&f.sponsor), Some(rules));
}

#[test]
fn rules_outside_what_the_desk_accepts_are_rejected() {
    let f = setup();

    let mut bad = rules(&f.env, ONE_FOR_ONE, 1_000);
    bad.match_bps = 0;
    assert_eq!(
        f.desk.try_set_rules(&f.sponsor, &bad),
        Err(Ok(SponsorError::InvalidRules))
    );

    let mut bad = rules(&f.env, ONE_FOR_ONE, 1_000);
    bad.match_bps = MAX_MATCH_BPS + 1;
    assert_eq!(
        f.desk.try_set_rules(&f.sponsor, &bad),
        Err(Ok(SponsorError::InvalidRules))
    );

    let mut bad = rules(&f.env, ONE_FOR_ONE, 1_000);
    bad.cap_per_voucher = 0;
    assert_eq!(
        f.desk.try_set_rules(&f.sponsor, &bad),
        Err(Ok(SponsorError::InvalidRules))
    );

    let mut bad = rules(&f.env, ONE_FOR_ONE, 1_000);
    bad.min_voucher_amount = -1;
    assert_eq!(
        f.desk.try_set_rules(&f.sponsor, &bad),
        Err(Ok(SponsorError::InvalidRules))
    );

    let mut bad = rules(&f.env, ONE_FOR_ONE, 1_000);
    bad.eligible_countries = vec![&f.env, String::from_str(&f.env, "NGA")];
    assert_eq!(
        f.desk.try_set_rules(&f.sponsor, &bad),
        Err(Ok(SponsorError::InvalidRules))
    );
}

// ----- matching -----

#[test]
fn a_match_creates_a_linked_voucher_for_the_same_care() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &BUDGET);
    f.desk
        .set_rules(&f.sponsor, &rules(&f.env, ONE_FOR_ONE, CAP));

    let original = fund(&f);
    let matched = f.desk.match_voucher(&f.sponsor, &original);

    // The matching voucher is an ordinary voucher for the same care.
    let summary = f.voucher.voucher_summary(&matched).unwrap();
    let original_summary = f.voucher.voucher_summary(&original).unwrap();
    assert_eq!(summary.0, original_summary.0, "same beneficiary");
    assert_eq!(summary.2, original_summary.2, "same clinic");
    assert_eq!(summary.3, original_summary.3, "same service");
    assert_eq!(summary.4, CONSULT_PRICE, "matched for the same amount");
    assert_eq!(summary.7, original_summary.7, "expires with the original");

    // It is funded by the desk, and the money is in the voucher's escrow.
    assert_eq!(summary.1, f.desk_addr, "the desk funded the match");
    assert_eq!(f.voucher.escrow_summary().0, 2 * CONSULT_PRICE);

    // The two are linked both ways.
    let record = f.desk.get_match(&original).unwrap();
    assert_eq!(record.original_voucher_id, original);
    assert_eq!(record.matched_voucher_id, matched);
    assert_eq!(record.amount, CONSULT_PRICE);
    assert_eq!(record.sponsor, f.sponsor);
    let back = f.desk.get_match_of(&matched).unwrap();
    assert_eq!(back, record);
}

#[test]
fn a_match_spends_the_budget_and_not_a_penny_more() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &BUDGET);
    f.desk
        .set_rules(&f.sponsor, &rules(&f.env, ONE_FOR_ONE, CAP));

    let original = fund(&f);
    f.desk.match_voucher(&f.sponsor, &original);

    assert_eq!(f.desk.available_budget(&f.sponsor), BUDGET - CONSULT_PRICE);
    let account = f.desk.get_sponsor(&f.sponsor);
    assert_eq!(account.deposited, BUDGET);
    assert_eq!(account.committed, CONSULT_PRICE);
    assert_eq!(account.withdrawn, 0);
}

#[test]
fn the_cap_limits_what_one_voucher_costs() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &BUDGET);
    // Ten times the voucher's amount, but only 4_000_000 is allowed.
    f.desk
        .set_rules(&f.sponsor, &rules(&f.env, 100_000, 4_000_000));

    let original = fund(&f);
    let matched = f.desk.match_voucher(&f.sponsor, &original);

    let summary = f.voucher.voucher_summary(&matched).unwrap();
    assert_eq!(summary.4, 4_000_000, "the match is capped");
    assert_eq!(f.desk.available_budget(&f.sponsor), BUDGET - 4_000_000);
}

#[test]
fn the_ratio_decides_how_much_is_added() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &BUDGET);
    f.desk.set_rules(&f.sponsor, &rules(&f.env, 20_000, CAP));

    let original = fund(&f);
    let matched = f.desk.match_voucher(&f.sponsor, &original);

    let summary = f.voucher.voucher_summary(&matched).unwrap();
    assert_eq!(summary.4, 2 * CONSULT_PRICE, "twice the funded amount");
}

#[test]
fn a_match_below_the_service_price_is_refused() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &BUDGET);
    // Half of the voucher, which is less than the price of the care.
    f.desk.set_rules(&f.sponsor, &rules(&f.env, 5_000, CAP));

    let original = fund(&f);
    assert_eq!(
        f.desk.try_match_voucher(&f.sponsor, &original),
        Err(Ok(SponsorError::MatchBelowPrice))
    );
}

#[test]
fn a_sponsor_cannot_match_its_own_voucher() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &BUDGET);
    f.desk
        .set_rules(&f.sponsor, &rules(&f.env, ONE_FOR_ONE, CAP));

    let own = f.voucher.create_voucher(
        &f.sponsor,
        &patient(&f.env),
        &f.provider,
        &CONSULT,
        &CONSULT_PRICE,
        &(START_TIME + 30 * DAY),
    );
    assert_eq!(
        f.desk.try_match_voucher(&f.sponsor, &own),
        Err(Ok(SponsorError::SelfMatchNotAllowed))
    );
}

#[test]
fn a_voucher_can_only_be_matched_once() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &BUDGET);
    f.desk
        .set_rules(&f.sponsor, &rules(&f.env, ONE_FOR_ONE, CAP));

    let original = fund(&f);
    f.desk.match_voucher(&f.sponsor, &original);
    assert_eq!(
        f.desk.try_match_voucher(&f.sponsor, &original),
        Err(Ok(SponsorError::AlreadyMatched))
    );
}

#[test]
fn a_voucher_that_does_not_exist_cannot_be_matched() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &BUDGET);
    f.desk
        .set_rules(&f.sponsor, &rules(&f.env, ONE_FOR_ONE, CAP));

    assert_eq!(
        f.desk.try_match_voucher(&f.sponsor, &999),
        Err(Ok(SponsorError::VoucherNotFound))
    );
}

#[test]
fn a_claimed_voucher_cannot_be_matched() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &BUDGET);
    f.desk
        .set_rules(&f.sponsor, &rules(&f.env, ONE_FOR_ONE, CAP));

    let original = fund(&f);
    f.voucher.claim(&f.provider, &original);
    assert_eq!(
        f.desk.try_match_voucher(&f.sponsor, &original),
        Err(Ok(SponsorError::VoucherNotFunded))
    );
}

#[test]
fn an_expired_voucher_cannot_be_matched() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &BUDGET);
    f.desk
        .set_rules(&f.sponsor, &rules(&f.env, ONE_FOR_ONE, CAP));

    let original = f.voucher.create_voucher(
        &f.funder,
        &patient(&f.env),
        &f.provider,
        &CONSULT,
        &CONSULT_PRICE,
        &(START_TIME + 60),
    );
    advance(&f.env, 61);
    assert_eq!(
        f.desk.try_match_voucher(&f.sponsor, &original),
        Err(Ok(SponsorError::VoucherExpired))
    );
}

#[test]
fn a_voucher_funded_too_recently_does_not_qualify() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &BUDGET);
    let mut rules = rules(&f.env, ONE_FOR_ONE, CAP);
    rules.min_voucher_age_secs = DAY;
    f.desk.set_rules(&f.sponsor, &rules);

    let original = fund(&f);
    assert_eq!(
        f.desk.try_match_voucher(&f.sponsor, &original),
        Err(Ok(SponsorError::VoucherTooNew))
    );

    // A day later it qualifies.
    advance(&f.env, DAY);
    f.desk.match_voucher(&f.sponsor, &original);
}

#[test]
fn a_voucher_below_the_minimum_does_not_qualify() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &BUDGET);
    let mut rules = rules(&f.env, ONE_FOR_ONE, CAP);
    rules.min_voucher_amount = CONSULT_PRICE * 2;
    f.desk.set_rules(&f.sponsor, &rules);

    let original = fund(&f);
    assert_eq!(
        f.desk.try_match_voucher(&f.sponsor, &original),
        Err(Ok(SponsorError::VoucherTooSmall))
    );
}

#[test]
fn a_clinic_outside_the_sponsors_countries_does_not_qualify() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &BUDGET);
    let mut rules = rules(&f.env, ONE_FOR_ONE, CAP);
    rules.eligible_countries = vec![&f.env, String::from_str(&f.env, "KE")];
    f.desk.set_rules(&f.sponsor, &rules);

    let original = fund(&f);
    assert_eq!(
        f.desk.try_match_voucher(&f.sponsor, &original),
        Err(Ok(SponsorError::CountryNotEligible))
    );
}

#[test]
fn a_clinic_in_one_of_the_sponsors_countries_qualifies() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &BUDGET);
    let mut rules = rules(&f.env, ONE_FOR_ONE, CAP);
    rules.eligible_countries = vec![
        &f.env,
        String::from_str(&f.env, "KE"),
        String::from_str(&f.env, "NG"),
    ];
    f.desk.set_rules(&f.sponsor, &rules);

    let original = fund(&f);
    f.desk.match_voucher(&f.sponsor, &original);
}

#[test]
fn a_service_outside_the_sponsors_list_does_not_qualify() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &BUDGET);
    let mut rules = rules(&f.env, ONE_FOR_ONE, CAP);
    rules.eligible_service_codes = vec![&f.env, 999u32];
    f.desk.set_rules(&f.sponsor, &rules);

    let original = fund(&f);
    assert_eq!(
        f.desk.try_match_voucher(&f.sponsor, &original),
        Err(Ok(SponsorError::ServiceNotEligible))
    );
}

#[test]
fn a_service_on_the_sponsors_list_qualifies() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &BUDGET);
    let mut rules = rules(&f.env, ONE_FOR_ONE, CAP);
    rules.eligible_service_codes = vec![&f.env, CONSULT];
    f.desk.set_rules(&f.sponsor, &rules);

    let original = fund(&f);
    f.desk.match_voucher(&f.sponsor, &original);
}

#[test]
fn a_sponsor_without_rules_matches_nothing() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &BUDGET);

    let original = fund(&f);
    assert_eq!(
        f.desk.try_match_voucher(&f.sponsor, &original),
        Err(Ok(SponsorError::RulesNotSet))
    );
}

#[test]
fn a_sponsor_cannot_match_a_voucher_the_desk_funded() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &BUDGET);
    f.desk
        .set_rules(&f.sponsor, &rules(&f.env, ONE_FOR_ONE, CAP));

    let original = fund(&f);
    let matched = f.desk.match_voucher(&f.sponsor, &original);

    // The matching voucher is funded by the desk, so it cannot be matched
    // again: that would pay the payer of the match.
    assert_eq!(
        f.desk.try_match_voucher(&f.sponsor, &matched),
        Err(Ok(SponsorError::SelfMatchNotAllowed))
    );
}

#[test]
fn a_clinic_that_is_no_longer_active_cannot_be_matched() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &BUDGET);
    f.desk
        .set_rules(&f.sponsor, &rules(&f.env, ONE_FOR_ONE, CAP));

    let original = fund(&f);
    f.registry
        .set_provider_status(&f.admin, &f.provider, &ProviderStatus::Suspended);
    assert_eq!(
        f.desk.try_match_voucher(&f.sponsor, &original),
        Err(Ok(SponsorError::ProviderNotEligible))
    );
}

#[test]
fn a_paused_service_cannot_be_matched() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &BUDGET);
    f.desk
        .set_rules(&f.sponsor, &rules(&f.env, ONE_FOR_ONE, CAP));

    let original = fund(&f);
    f.registry
        .set_service_available(&f.provider, &CONSULT, &false);
    assert_eq!(
        f.desk.try_match_voucher(&f.sponsor, &original),
        Err(Ok(SponsorError::ServiceUnavailable))
    );
}

#[test]
fn a_service_the_clinic_delisted_cannot_be_matched() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &BUDGET);
    f.desk
        .set_rules(&f.sponsor, &rules(&f.env, ONE_FOR_ONE, CAP));

    let original = fund(&f);
    f.registry.remove_service(&f.provider, &CONSULT);
    assert_eq!(
        f.desk.try_match_voucher(&f.sponsor, &original),
        Err(Ok(SponsorError::ServiceNotEligible))
    );
}

#[test]
fn a_match_of_nothing_is_refused() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &BUDGET);
    // A hundredth of a percent of a cheap voucher rounds down to nothing.
    f.desk.set_rules(&f.sponsor, &rules(&f.env, 1, CAP));

    let cheap = f.voucher.create_voucher(
        &f.funder,
        &patient(&f.env),
        &f.provider,
        &CHEAP,
        &CHEAP_PRICE,
        &(START_TIME + 30 * DAY),
    );
    assert_eq!(
        f.desk.try_match_voucher(&f.sponsor, &cheap),
        Err(Ok(SponsorError::MatchTooSmall))
    );
}

// ----- budget -----

#[test]
fn the_budget_runs_out() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &CONSULT_PRICE);
    f.desk
        .set_rules(&f.sponsor, &rules(&f.env, ONE_FOR_ONE, CAP));

    let first = fund(&f);
    f.desk.match_voucher(&f.sponsor, &first);
    assert_eq!(f.desk.available_budget(&f.sponsor), 0);

    let second = fund(&f);
    assert_eq!(
        f.desk.try_match_voucher(&f.sponsor, &second),
        Err(Ok(SponsorError::BudgetExhausted))
    );
}

#[test]
fn a_sponsor_cannot_match_more_than_its_budget() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &(CONSULT_PRICE - 1));
    f.desk
        .set_rules(&f.sponsor, &rules(&f.env, ONE_FOR_ONE, CAP));

    let original = fund(&f);
    assert_eq!(
        f.desk.try_match_voucher(&f.sponsor, &original),
        Err(Ok(SponsorError::BudgetExhausted))
    );
}

#[test]
fn too_many_open_matches_are_refused() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &1_000_000_000);
    f.desk
        .set_rules(&f.sponsor, &rules(&f.env, ONE_FOR_ONE, CAP));

    for _ in 0..MAX_OPEN_MATCHES {
        let original = fund(&f);
        f.desk.match_voucher(&f.sponsor, &original);
    }
    let original = fund(&f);
    assert_eq!(
        f.desk.try_match_voucher(&f.sponsor, &original),
        Err(Ok(SponsorError::TooManyOpenMatches))
    );
}

// ----- withdrawal -----

#[test]
fn a_sponsor_withdraws_its_unused_budget() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &BUDGET);
    f.desk
        .set_rules(&f.sponsor, &rules(&f.env, ONE_FOR_ONE, CAP));

    let original = fund(&f);
    f.desk.match_voucher(&f.sponsor, &original);

    // The match is escrowed, so only the rest is the sponsor's to take.
    f.desk.withdraw(&f.sponsor, &(BUDGET - CONSULT_PRICE));
    assert_eq!(f.desk.available_budget(&f.sponsor), 0);
    assert_eq!(
        f.token.balance(&f.sponsor),
        1_000_000_000 - BUDGET + BUDGET - CONSULT_PRICE
    );
}

#[test]
fn a_sponsor_cannot_withdraw_money_it_matched() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &BUDGET);
    f.desk
        .set_rules(&f.sponsor, &rules(&f.env, ONE_FOR_ONE, CAP));

    let original = fund(&f);
    f.desk.match_voucher(&f.sponsor, &original);

    assert_eq!(
        f.desk.try_withdraw(&f.sponsor, &BUDGET),
        Err(Ok(SponsorError::WithdrawExceedsBudget))
    );
}

#[test]
fn a_withdrawal_of_nothing_is_refused() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &BUDGET);
    assert_eq!(
        f.desk.try_withdraw(&f.sponsor, &0),
        Err(Ok(SponsorError::InvalidAmount))
    );
}

// ----- refunds and settlement -----

#[test]
fn a_refunded_match_gives_the_budget_back() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &BUDGET);
    f.desk
        .set_rules(&f.sponsor, &rules(&f.env, ONE_FOR_ONE, CAP));

    let original = fund(&f);
    let matched = f.desk.match_voucher(&f.sponsor, &original);
    assert_eq!(f.desk.available_budget(&f.sponsor), BUDGET - CONSULT_PRICE);

    // The care never happened, so the matching voucher is refunded and the
    // money comes back to the desk.
    advance(&f.env, 31 * DAY);
    f.voucher.refund(&matched);

    // The books are only up to date once they are reconciled.
    f.desk.reconcile_sponsor(&f.sponsor);
    assert_eq!(f.desk.available_budget(&f.sponsor), BUDGET);
    let account = f.desk.get_sponsor(&f.sponsor);
    assert_eq!(account.refunded, CONSULT_PRICE);
    assert!(account.open_matches.is_empty());
}

#[test]
fn a_settled_match_stays_spent() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &BUDGET);
    f.desk
        .set_rules(&f.sponsor, &rules(&f.env, ONE_FOR_ONE, CAP));

    let original = fund(&f);
    let matched = f.desk.match_voucher(&f.sponsor, &original);

    // The clinic delivers and an attester confirms it.
    f.voucher.claim(&f.provider, &matched);
    f.voucher.attest(&f.attester, &matched);
    // Settlement waits for the dispute window to close.
    advance(&f.env, 73 * 60 * 60);
    f.voucher.settle(&matched);

    f.desk.reconcile_sponsor(&f.sponsor);
    assert_eq!(f.desk.available_budget(&f.sponsor), BUDGET - CONSULT_PRICE);
    let account = f.desk.get_sponsor(&f.sponsor);
    assert_eq!(account.refunded, 0);
    assert!(account.open_matches.is_empty());
}

// ----- administration -----

#[test]
fn the_desk_can_be_paused_and_unpaused() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &1_000);
    f.desk
        .set_rules(&f.sponsor, &rules(&f.env, ONE_FOR_ONE, CAP));
    f.desk.set_paused(&f.admin, &true);

    assert_eq!(
        f.desk.try_deposit(&f.sponsor, &1_000),
        Err(Ok(SponsorError::Paused))
    );
    let original = fund(&f);
    assert_eq!(
        f.desk.try_match_voucher(&f.sponsor, &original),
        Err(Ok(SponsorError::Paused))
    );

    // Withdrawals keep working while paused: money already here is not
    // trapped.
    f.desk.withdraw(&f.sponsor, &1_000);
    f.desk.set_paused(&f.admin, &false);
    f.desk.deposit(&f.sponsor, &1_000);
}

#[test]
fn only_the_admin_can_pause_the_desk() {
    let f = setup();
    let stranger = Address::generate(&f.env);
    assert_eq!(
        f.desk.try_set_paused(&stranger, &true),
        Err(Ok(SponsorError::NotAuthorized))
    );
}

#[test]
fn the_admin_role_moves_in_two_steps() {
    let f = setup();
    let new_admin = Address::generate(&f.env);

    f.desk.propose_admin(&f.admin, &new_admin);
    assert_eq!(f.desk.get_pending_admin(), Some(new_admin.clone()));
    // The old admin is still the admin until the nomination is accepted.
    assert_eq!(f.desk.get_config().admin, f.admin);

    f.desk.accept_admin(&new_admin);
    assert_eq!(f.desk.get_config().admin, new_admin);
    assert_eq!(f.desk.get_pending_admin(), None);
}

#[test]
fn accepting_an_admin_nomination_needs_the_nominee() {
    let f = setup();
    let new_admin = Address::generate(&f.env);
    let stranger = Address::generate(&f.env);

    f.desk.propose_admin(&f.admin, &new_admin);
    assert_eq!(
        f.desk.try_accept_admin(&stranger),
        Err(Ok(SponsorError::NotAuthorized))
    );
}

#[test]
fn there_is_no_admin_to_accept_without_a_nomination() {
    let f = setup();
    let new_admin = Address::generate(&f.env);
    assert_eq!(
        f.desk.try_accept_admin(&new_admin),
        Err(Ok(SponsorError::NoPendingAdmin))
    );
}

#[test]
fn a_desk_cannot_be_initialised_twice() {
    let f = setup();
    assert_eq!(
        f.desk
            .try_initialize(&f.admin, &f.registry_addr, &f.voucher_addr, &f.token_addr,),
        Err(Ok(SponsorError::AlreadyInitialized))
    );
}

// ----- views -----

#[test]
fn an_unknown_sponsor_has_an_empty_account() {
    let f = setup();
    let stranger = Address::generate(&f.env);
    let account = f.desk.get_sponsor(&stranger);
    assert_eq!(account.deposited, 0);
    assert_eq!(account.sponsor, stranger);
    assert_eq!(f.desk.get_rules(&stranger), None);
    assert_eq!(f.desk.available_budget(&stranger), 0);
}

#[test]
fn the_version_is_the_crate_version() {
    let f = setup();
    assert_eq!(
        f.desk.version(),
        String::from_str(&f.env, env!("CARGO_PKG_VERSION"))
    );
}

// ----- authorisation matrix -----
//
// One row per public function: the exact signer it must demand, and nothing
// else. The host records one entry per address a `require_auth()` was made
// for during an invocation, so comparing `env.auths()` with the expected
// signer catches both kinds of mistake:
//
//   * a missing `require_auth()` — anyone could spend another party's
//     budget, and
//   * an extra `require_auth()` — a call that a relayer could have made
//     would start needing a signature.
//
// The table in `docs/developers/contracts.md` is the rendered form of this
// same list, and `authorisation_table_in_the_docs_matches_the_matrix` fails
// when the two drift.
//
// What breaks if this is wrong: `reconcile_sponsor` is deliberately
// permissionless — it only brings the books up to date — while every call
// that moves money has to be signed by the party whose money it is.

use soroban_sdk::xdr::{ContractEvent, ContractEventBody, ScVal};

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

fn sponsor_addr(f: &AuthFixture) -> Address {
    f.f.sponsor.clone()
}

fn admin_addr(f: &AuthFixture) -> Address {
    f.f.admin.clone()
}

fn new_admin_addr(f: &AuthFixture) -> Address {
    f.new_admin.clone()
}

/// Every public function of the sponsorship desk, in declaration order.
const AUTH_MATRIX: &[AuthCase] = &[
    AuthCase {
        function: "initialize",
        auth: Auth::Free("none (one-shot)"),
    },
    AuthCase {
        function: "deposit",
        auth: signer("`sponsor`", sponsor_addr),
    },
    AuthCase {
        function: "set_rules",
        auth: signer("`sponsor`", sponsor_addr),
    },
    AuthCase {
        function: "match_voucher",
        auth: signer("`sponsor`", sponsor_addr),
    },
    AuthCase {
        function: "withdraw",
        auth: signer("`sponsor`", sponsor_addr),
    },
    AuthCase {
        function: "reconcile_sponsor",
        auth: Auth::Free("none (permissionless)"),
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
        function: "set_paused",
        auth: signer("`admin`", admin_addr),
    },
    AuthCase {
        function: "get_config",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "get_sponsor",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "get_rules",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "available_budget",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "get_match",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "get_match_of",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "budget_summary",
        auth: Auth::Free("none (view)"),
    },
    AuthCase {
        function: "get_pending_admin",
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

/// Runs `function` in the state that makes it a valid call, so the only thing
/// under test is who it demands. Every arm leaves the function under test as
/// the last invocation, which is what `env.auths()` reports.
fn run_case(f: &AuthFixture, function: &str) -> std::vec::Vec<Address> {
    let desk = &f.f.desk;
    let env = &f.f.env;

    match function {
        // One-shot setup needs a fresh contract, so it cannot share the
        // fixture: assert on an env that has never been initialised.
        "initialize" => {
            let env = Env::default();
            env.mock_all_auths();
            let a = Address::generate(&env);
            let address = env.register(SponsorDesk, ());
            SponsorDeskClient::new(&env, &address).initialize(&a, &a, &a, &a);
            return recorded_auths(&env);
        }
        "deposit" => {
            desk.deposit(&f.f.sponsor, &BUDGET);
        }
        "set_rules" => {
            desk.set_rules(&f.f.sponsor, &rules(env, ONE_FOR_ONE, CAP));
        }
        "match_voucher" => {
            desk.deposit(&f.f.sponsor, &BUDGET);
            desk.set_rules(&f.f.sponsor, &rules(env, ONE_FOR_ONE, CAP));
            let id = fund(&f.f);
            desk.match_voucher(&f.f.sponsor, &id);
        }
        "withdraw" => {
            desk.deposit(&f.f.sponsor, &BUDGET);
            desk.withdraw(&f.f.sponsor, &BUDGET);
        }
        "reconcile_sponsor" => {
            desk.reconcile_sponsor(&f.f.sponsor);
        }
        "propose_admin" => {
            desk.propose_admin(&f.f.admin, &f.new_admin);
        }
        "accept_admin" => {
            desk.propose_admin(&f.f.admin, &f.new_admin);
            desk.accept_admin(&f.new_admin);
        }
        "set_paused" => {
            desk.set_paused(&f.f.admin, &true);
        }
        "get_config" => {
            desk.get_config();
        }
        "get_sponsor" => {
            desk.get_sponsor(&f.f.sponsor);
        }
        "get_rules" => {
            desk.get_rules(&f.f.sponsor);
        }
        "available_budget" => {
            desk.available_budget(&f.f.sponsor);
        }
        "get_match" => {
            desk.get_match(&1);
        }
        "get_match_of" => {
            desk.get_match_of(&1);
        }
        "budget_summary" => {
            desk.budget_summary();
        }
        "get_pending_admin" => {
            desk.get_pending_admin();
        }
        "version" => {
            desk.version();
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

    let rows = doc_rows(DOCS, "sponsor");
    assert_eq!(
        rows.len(),
        AUTH_MATRIX.len(),
        "docs/developers/contracts.md documents {} sponsor functions but the matrix has {}",
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
// Every event this contract can publish is pinned here: the topic list and
// the exact set of data fields. A change to any of them fails with a message
// naming the decoder that would break, because the indexer in
// sci-healthcare-backend reads these by name.

/// The event's topics, as published (topic 0 is the event name).
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

fn field(e: &ContractEvent, name: &str) -> ScVal {
    let entries = data(e).unwrap_or_else(|| panic!("event has no data map: {e:?}"));
    for entry in entries.iter() {
        if symbol(&entry.key) == Some(name) {
            return entry.val.clone();
        }
    }
    panic!("event has no field `{name}`: {e:?}");
}

/// The declared fields present on the event, in the order the data map
/// carries them (an XDR map is sorted by key, so this is stable).
fn field_names(e: &ContractEvent) -> std::vec::Vec<&str> {
    let entries = data(e).unwrap_or_else(|| panic!("event data is not a map: {e:?}"));
    entries
        .iter()
        .filter_map(|entry| symbol(&entry.key))
        .collect()
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

fn field_address(env: &Env, e: &ContractEvent, name: &str) -> Address {
    use soroban_sdk::TryFromVal;

    let value = field(e, name);
    match &value {
        ScVal::Address(_) => Address::try_from_val(env, &value)
            .unwrap_or_else(|_| panic!("event field `{name}` is not a usable address")),
        other => panic!("event field `{name}` is {other:?}, want an address"),
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

/// The desk's own events, so the voucher's events do not get in the way.
fn desk_events(f: &Fixture) -> std::vec::Vec<ContractEvent> {
    let events = f.env.events().all().filter_by_contract(&f.desk.address);
    events.events().to_vec()
}

#[test]
fn initialized_event_snapshot() {
    let env = Env::default();
    env.mock_all_auths();
    let a = Address::generate(&env);
    let address = env.register(SponsorDesk, ());
    SponsorDeskClient::new(&env, &address).initialize(&a, &a, &a, &a);

    let events = env.events().all().filter_by_contract(&address);
    let list = events.events();
    assert_eq!(list.len(), 1, "initialize publishes one event");
    let e = list[0].clone();

    assert_layout(&e, "initialized", &[], &["admin"]);
    assert_eq!(field_address(&env, &e, "admin"), a);
}

#[test]
fn budget_deposited_event_snapshot() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &BUDGET);

    let list = desk_events(&f);
    assert_eq!(list.len(), 1, "deposit publishes one event");
    let e = list[0].clone();

    assert_layout(
        &e,
        "budget_deposited",
        &[ScVal::from(&f.sponsor)],
        &["amount", "available"],
    );
    assert_eq!(field_i128(&e, "amount"), BUDGET);
    assert_eq!(field_i128(&e, "available"), BUDGET);
}

#[test]
fn rules_set_event_snapshot() {
    let f = setup();
    f.desk
        .set_rules(&f.sponsor, &rules(&f.env, ONE_FOR_ONE, CAP));

    let list = desk_events(&f);
    assert_eq!(list.len(), 1, "set_rules publishes one event");
    let e = list[0].clone();

    assert_layout(
        &e,
        "rules_set",
        &[ScVal::from(&f.sponsor)],
        &[
            "match_bps",
            "cap_per_voucher",
            "min_voucher_amount",
            "min_voucher_age_secs",
        ],
    );
    assert_eq!(field_u32(&e, "match_bps"), ONE_FOR_ONE);
    assert_eq!(field_i128(&e, "cap_per_voucher"), CAP);
    assert_eq!(field_i128(&e, "min_voucher_amount"), 0);
    assert_eq!(field_u64(&e, "min_voucher_age_secs"), 0);
}

#[test]
fn match_created_event_snapshot() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &BUDGET);
    f.desk
        .set_rules(&f.sponsor, &rules(&f.env, ONE_FOR_ONE, CAP));
    let original = fund(&f);
    let matched = f.desk.match_voucher(&f.sponsor, &original);

    let list = desk_events(&f);
    assert_eq!(list.len(), 1, "a match publishes one event");
    let e = list[0].clone();

    assert_layout(
        &e,
        "match_created",
        &[ScVal::from(&f.sponsor), ScVal::U64(original)],
        &[
            "matched_voucher_id",
            "provider",
            "service_code",
            "amount",
            "matched_at",
        ],
    );
    assert_eq!(field_u64(&e, "matched_voucher_id"), matched);
    assert_eq!(field_address(&f.env, &e, "provider"), f.provider);
    assert_eq!(field_u32(&e, "service_code"), CONSULT);
    assert_eq!(field_i128(&e, "amount"), CONSULT_PRICE);
    assert_eq!(field_u64(&e, "matched_at"), START_TIME);
}

#[test]
fn match_settled_and_refunded_event_snapshots() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &BUDGET);
    f.desk
        .set_rules(&f.sponsor, &rules(&f.env, ONE_FOR_ONE, CAP));
    let original = fund(&f);
    let matched = f.desk.match_voucher(&f.sponsor, &original);

    // The care never happened: the matching voucher is refunded and the
    // money comes back.
    advance(&f.env, 31 * DAY);
    f.voucher.refund(&matched);
    f.desk.reconcile_sponsor(&f.sponsor);

    let list = desk_events(&f);
    let refunded = list
        .iter()
        .find(|e| symbol(&topics(e)[0]) == Some("match_refunded"))
        .expect("the refund publishes an event");
    assert_layout(
        refunded,
        "match_refunded",
        &[ScVal::from(&f.sponsor)],
        &["matched_voucher_id", "amount"],
    );
    assert_eq!(field_u64(refunded, "matched_voucher_id"), matched);
    assert_eq!(field_i128(refunded, "amount"), CONSULT_PRICE);

    // A settled match publishes the other event.
    let f = setup();
    f.desk.deposit(&f.sponsor, &BUDGET);
    f.desk
        .set_rules(&f.sponsor, &rules(&f.env, ONE_FOR_ONE, CAP));
    let original = fund(&f);
    let matched = f.desk.match_voucher(&f.sponsor, &original);
    f.voucher.claim(&f.provider, &matched);
    f.voucher.attest(&f.attester, &matched);
    advance(&f.env, 73 * 60 * 60);
    f.voucher.settle(&matched);
    f.desk.reconcile_sponsor(&f.sponsor);

    let list = desk_events(&f);
    let settled = list
        .iter()
        .find(|e| symbol(&topics(e)[0]) == Some("match_settled"))
        .expect("the settlement publishes an event");
    assert_layout(
        settled,
        "match_settled",
        &[ScVal::from(&f.sponsor)],
        &["matched_voucher_id", "amount"],
    );
    assert_eq!(field_u64(settled, "matched_voucher_id"), matched);
    assert_eq!(field_i128(settled, "amount"), CONSULT_PRICE);
}

#[test]
fn budget_withdrawn_event_snapshot() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &BUDGET);
    f.desk.withdraw(&f.sponsor, &1_000);

    let list = desk_events(&f);
    let withdrawn = list
        .iter()
        .find(|e| symbol(&topics(e)[0]) == Some("budget_withdrawn"))
        .expect("the withdrawal publishes an event");
    assert_layout(
        withdrawn,
        "budget_withdrawn",
        &[ScVal::from(&f.sponsor)],
        &["amount", "available"],
    );
    assert_eq!(field_i128(withdrawn, "amount"), 1_000);
    assert_eq!(field_i128(withdrawn, "available"), BUDGET - 1_000);
}

#[test]
fn admin_events_snapshot() {
    let f = setup();
    let new_admin = Address::generate(&f.env);
    f.desk.propose_admin(&f.admin, &new_admin);

    let list = desk_events(&f);
    let proposed = list
        .iter()
        .find(|e| symbol(&topics(e)[0]) == Some("admin_proposed"))
        .expect("the nomination publishes an event");
    assert_layout(
        proposed,
        "admin_proposed",
        &[ScVal::from(&f.admin)],
        &["pending_admin"],
    );
    assert_eq!(field_address(&f.env, proposed, "pending_admin"), new_admin);

    f.desk.accept_admin(&new_admin);
    let list = desk_events(&f);
    let changed = list
        .iter()
        .find(|e| symbol(&topics(e)[0]) == Some("admin_changed"))
        .expect("the handover publishes an event");
    assert_layout(changed, "admin_changed", &[], &["new_admin"]);
    assert_eq!(field_address(&f.env, changed, "new_admin"), new_admin);
}

// ----- storage lifetime (TTL) -----
//
// Soroban archives persistent entries whose TTL lapses. This contract extends
// TTL on every sponsor account it reads or writes, and on every instance
// write, so a sponsor that comes back after a quiet month finds its budget
// and its rules where it left them.

const DAY_IN_LEDGERS: u32 = 17_280;
const SEVEN_DAYS: u32 = 7 * DAY_IN_LEDGERS;
const THIRTY_DAYS: u32 = 30 * DAY_IN_LEDGERS;
const NINETY_DAYS: u32 = 90 * DAY_IN_LEDGERS;

fn advance_days(f: &Fixture, days: u32) {
    f.env.ledger().with_mut(|li| {
        li.sequence_number += days * DAY_IN_LEDGERS;
        li.timestamp += u64::from(days) * 24 * 60 * 60;
    });
}

fn instance_ttl(f: &Fixture) -> u32 {
    let contract = f.desk.address.clone();
    f.env
        .as_contract(&contract, || f.env.storage().instance().get_ttl())
}

fn sponsor_ttl(f: &Fixture) -> u32 {
    let contract = f.desk.address.clone();
    let key = crate::DataKey::Sponsor(f.sponsor.clone());
    f.env
        .as_contract(&contract, || f.env.storage().persistent().get_ttl(&key))
}

#[test]
fn a_sponsor_account_survives_sixty_days_of_activity() {
    let f = setup();
    f.desk.deposit(&f.sponsor, &BUDGET);
    f.desk
        .set_rules(&f.sponsor, &rules(&f.env, ONE_FOR_ONE, CAP));

    // Ten days at a time, the way a wallet or the indexer polls.
    for day in 1..=6 {
        advance_days(&f, 10);
        assert_eq!(f.desk.available_budget(&f.sponsor), BUDGET);
        // Reads extend the entry once it falls below the 30-day threshold,
        // so it never gets close to lapsing (see "Storage lifetime" in
        // docs/developers/contracts.md).
        let ttl = sponsor_ttl(&f);
        assert!(
            (THIRTY_DAYS..=NINETY_DAYS).contains(&ttl),
            "the sponsor's account had {ttl} ledgers left on day {}",
            day * 10,
        );
    }
}

#[test]
fn writes_extend_the_contract_instance() {
    let f = setup();
    // Setup left it with a 30-day TTL; 25 days of silence leaves five.
    advance_days(&f, 25);
    assert!(instance_ttl(&f) < SEVEN_DAYS);

    f.desk.deposit(&f.sponsor, &BUDGET);

    assert_eq!(instance_ttl(&f), THIRTY_DAYS);
}

// ----- the errors that are hard to reach -----

#[test]
fn not_initialized_errors() {
    let env = Env::default();
    env.mock_all_auths();
    let a = Address::generate(&env);
    let addr = env.register(SponsorDesk, ());
    let desk = SponsorDeskClient::new(&env, &addr);

    let err = desk.try_get_config().err().unwrap().unwrap();
    assert_eq!(err, SponsorError::NotInitialized);
    let err = desk.try_deposit(&a, &100).err().unwrap().unwrap();
    assert_eq!(err, SponsorError::NotInitialized);
    let err = desk.try_withdraw(&a, &100).err().unwrap().unwrap();
    assert_eq!(err, SponsorError::NotInitialized);
}

#[test]
fn a_desk_pointed_at_the_wrong_token_cannot_match() {
    let f = setup();
    // A second settlement token, which the voucher contract knows nothing
    // about: the money it moves is not the money the desk authorised.
    let issuer = Address::generate(&f.env);
    let other = f.env.register_stellar_asset_contract_v2(issuer);
    let desk_addr = f.env.register(SponsorDesk, ());
    let desk = SponsorDeskClient::new(&f.env, &desk_addr);
    desk.initialize(
        &f.admin,
        &f.registry_addr,
        &f.voucher_addr,
        &other.address(),
    );

    // The sponsor holds the other token, so the deposit succeeds and the
    // failure comes from the voucher contract, where it belongs.
    token::StellarAssetClient::new(&f.env, &other.address()).mint(&f.sponsor, &BUDGET);
    desk.deposit(&f.sponsor, &BUDGET);
    desk.set_rules(&f.sponsor, &rules(&f.env, ONE_FOR_ONE, CAP));
    let original = fund(&f);

    assert_eq!(
        desk.try_match_voucher(&f.sponsor, &original),
        Err(Ok(SponsorError::VoucherCallFailed))
    );
}

#[test]
fn a_budget_counter_that_would_overflow_is_refused() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = START_TIME);

    let admin = Address::generate(&env);
    let sponsor = Address::generate(&env);

    let issuer = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(issuer);
    let token_addr = sac.address();
    token::StellarAssetClient::new(&env, &token_addr).mint(&sponsor, &i128::MAX);

    let registry_addr = env.register(Registry, ());
    let registry = RegistryClient::new(&env, &registry_addr);
    registry.initialize(&admin);

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
        &(72 * 60 * 60),
        &100,
        &50,
        &admin,
    );

    let desk_addr = env.register(SponsorDesk, ());
    let desk = SponsorDeskClient::new(&env, &desk_addr);
    desk.initialize(&admin, &registry_addr, &voucher_addr, &token_addr);

    // The whole supply goes in, comes back, and goes in once more: the
    // running total of deposits would no longer fit in an i128.
    desk.deposit(&sponsor, &i128::MAX);
    desk.withdraw(&sponsor, &(i128::MAX - 1));
    let err = desk.try_deposit(&sponsor, &1).err().unwrap().unwrap();
    assert_eq!(err, SponsorError::MathOverflow);
}
