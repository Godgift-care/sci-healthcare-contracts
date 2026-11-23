//! The voucher contract assumes the settlement token moves exactly the amount
//! requested: USDC on Stellar does, but a token with transfer fees or hooks
//! does not. These tests swap the Stellar asset for a mock token that can
//! skim a fee from the recipient, overdraw the sender, or silently move
//! nothing, and prove the contract fails closed with `TokenAmountMismatch`
//! rather than recording escrow it does not hold.
//!
//! What breaks if this is wrong: a fee-charging token funds a voucher with
//! less than its recorded amount, `open_escrow` drifts above the real
//! balance, and the shortfall surfaces later as a clinic paid short or an
//! unrelated voucher that can no longer settle.

use soroban_sdk::{contract, contractimpl, contracttype, MuxedAddress};

use super::*;

/// Exact transfers, like a standard Stellar asset.
const EXACT: u32 = 0;
/// The recipient is credited `amount - fee`; the fee vanishes.
const SKIM_RECIPIENT: u32 = 1;
/// The sender is debited `amount + fee`; the recipient gets `amount`.
const OVERDRAW_SENDER: u32 = 2;
/// The call succeeds but no balance changes (a broken hook).
const NO_OP: u32 = 3;

#[contracttype]
enum MockKey {
    Balance(Address),
    Mode,
}

/// A deliberately non-standard token. Only the calls the voucher contract
/// makes (`balance`, `transfer`) plus test helpers are implemented.
#[contract]
pub struct MockFeeToken;

fn fee_of(amount: i128) -> i128 {
    (amount / 100).max(1)
}

#[contractimpl]
impl MockFeeToken {
    pub fn mint(env: Env, to: Address, amount: i128) {
        let bal = Self::balance(env.clone(), to.clone());
        env.storage()
            .instance()
            .set(&MockKey::Balance(to), &(bal + amount));
    }

    pub fn set_mode(env: Env, mode: u32) {
        env.storage().instance().set(&MockKey::Mode, &mode);
    }

    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage()
            .instance()
            .get(&MockKey::Balance(id))
            .unwrap_or(0)
    }

    pub fn transfer(env: Env, from: Address, to: MuxedAddress, amount: i128) {
        from.require_auth();
        let to = to.address();
        let mode: u32 = env
            .storage()
            .instance()
            .get(&MockKey::Mode)
            .unwrap_or(EXACT);
        let (debit, credit) = match mode {
            SKIM_RECIPIENT => (amount, amount - fee_of(amount)),
            OVERDRAW_SENDER => (amount + fee_of(amount), amount),
            NO_OP => (0, 0),
            _ => (amount, amount),
        };
        let from_bal = Self::balance(env.clone(), from.clone());
        assert!(from_bal >= debit, "insufficient balance");
        env.storage()
            .instance()
            .set(&MockKey::Balance(from), &(from_bal - debit));
        let to_bal = Self::balance(env.clone(), to.clone());
        env.storage()
            .instance()
            .set(&MockKey::Balance(to), &(to_bal + credit));
    }
}

struct TokenFixture {
    env: Env,
    voucher: VoucherEscrowClient<'static>,
    token: MockFeeTokenClient<'static>,
    funder: Address,
    provider: Address,
    attester: Address,
    fee_account: Address,
}

fn setup_mock_token() -> TokenFixture {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = START_TIME);

    let admin = Address::generate(&env);
    let funder = Address::generate(&env);
    let provider = Address::generate(&env);
    let attester = Address::generate(&env);
    let fee_account = Address::generate(&env);

    let token_addr = env.register(MockFeeToken, ());
    let token = MockFeeTokenClient::new(&env, &token_addr);
    token.mint(&funder, &1_000_000_000i128);

    let registry_addr = env.register(Registry, ());
    let registry = RegistryClient::new(&env, &registry_addr);
    registry.initialize(&admin);

    let voucher_addr = env.register(VoucherEscrow, ());
    let voucher = VoucherEscrowClient::new(&env, &voucher_addr);
    let receipt_addr = env.register(ReceiptBook, ());
    ReceiptBookClient::new(&env, &receipt_addr).initialize(&admin, &voucher_addr);

    voucher.initialize(
        &admin,
        &registry_addr,
        &receipt_addr,
        &token_addr,
        &DISPUTE_WINDOW,
        &FEE_BPS,
        &ATTESTER_SHARE_BPS,
        &fee_account,
    );

    registry.register_provider(
        &provider,
        &String::from_str(&env, "Ikeja General Clinic"),
        &String::from_str(&env, "NG"),
        &ProviderCategory::Clinic,
    );
    registry.set_provider_status(&admin, &provider, &ProviderStatus::Active, &REASON_VERIFIED);
    registry.upsert_service(
        &provider,
        &CONSULT,
        &String::from_str(&env, "Outpatient consult"),
        &CONSULT_PRICE,
    );
    registry.add_attester(&admin, &attester);

    TokenFixture {
        env,
        voucher,
        token,
        funder,
        provider,
        attester,
        fee_account,
    }
}

const EXPIRES_AT: u64 = START_TIME + 30 * 24 * 60 * 60;

fn try_fund(f: &TokenFixture) -> Result<u64, VoucherError> {
    match f.voucher.try_create_voucher(
        &f.funder,
        &patient(&f.env),
        &f.provider,
        &CONSULT,
        &CONSULT_PRICE,
        &EXPIRES_AT,
    ) {
        Ok(Ok(id)) => Ok(id),
        Err(Ok(e)) => Err(e),
        other => panic!("unexpected result {other:?}"),
    }
}

fn fund_attested(f: &TokenFixture) -> u64 {
    let id = try_fund(f).unwrap();
    f.voucher.claim(&f.provider, &id);
    f.voucher.attest(&f.attester, &id);
    id
}

/// `open_escrow` and the real balance agree and equal `expected`.
fn assert_escrow_is(f: &TokenFixture, expected: i128) {
    let (open_escrow, balance) = f.voucher.escrow_summary();
    assert_eq!(open_escrow, expected, "open_escrow is wrong");
    assert_eq!(balance, expected, "token balance is wrong");
}

fn err_of<T: core::fmt::Debug>(
    r: Result<
        Result<T, soroban_sdk::ConversionError>,
        Result<VoucherError, soroban_sdk::InvokeError>,
    >,
) -> VoucherError {
    r.err().unwrap().unwrap()
}

#[test]
fn exact_mock_token_runs_the_full_lifecycle() {
    // Baseline: the mock behaves like a standard token in EXACT mode, so the
    // failures below are caused by the fee modes and nothing else.
    let f = setup_mock_token();
    let id = fund_attested(&f);
    assert_escrow_is(&f, CONSULT_PRICE);

    advance(&f.env, DISPUTE_WINDOW + 1);
    f.voucher.settle(&id);

    let fee = CONSULT_PRICE * FEE_BPS as i128 / 10_000;
    let attester_share = CONSULT_PRICE * ATTESTER_SHARE_BPS as i128 / 10_000;
    assert_eq!(f.token.balance(&f.provider), CONSULT_PRICE - fee);
    assert_eq!(f.token.balance(&f.attester), attester_share);
    assert_eq!(f.token.balance(&f.fee_account), fee - attester_share);
    assert_escrow_is(&f, 0);
}

#[test]
fn short_delivering_token_cannot_fund_a_voucher() {
    for mode in [SKIM_RECIPIENT, NO_OP] {
        let f = setup_mock_token();
        f.token.set_mode(&mode);
        let funder_before = f.token.balance(&f.funder);

        assert_eq!(
            try_fund(&f),
            Err(VoucherError::TokenAmountMismatch),
            "mode {mode}"
        );

        // Nothing moved and nothing was recorded.
        assert_eq!(f.token.balance(&f.funder), funder_before);
        assert_escrow_is(&f, 0);
        assert_eq!(
            f.voucher.get_exposure(&f.provider),
            Exposure {
                vouchers: 0,
                value: 0
            }
        );
        assert!(f.voucher.try_get_voucher(&1).is_err(), "no voucher written");
    }
}

#[test]
fn overdrawing_token_still_funds_exactly() {
    // The funder pays a fee on top, but the escrow receives exactly the
    // voucher amount, so the escrow invariant holds and funding proceeds.
    let f = setup_mock_token();
    f.token.set_mode(&OVERDRAW_SENDER);
    try_fund(&f).unwrap();
    assert_escrow_is(&f, CONSULT_PRICE);
}

#[test]
fn short_delivering_token_cannot_settle() {
    for mode in [SKIM_RECIPIENT, OVERDRAW_SENDER, NO_OP] {
        let f = setup_mock_token();
        let id = fund_attested(&f);
        advance(&f.env, DISPUTE_WINDOW + 1);

        f.token.set_mode(&mode);
        assert_eq!(
            err_of(f.voucher.try_settle(&id)),
            VoucherError::TokenAmountMismatch,
            "mode {mode}"
        );

        // The whole call reverted: the voucher is still attested, the clinic
        // was paid nothing and the escrow still holds the full amount.
        assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Attested);
        assert_eq!(f.token.balance(&f.provider), 0);
        assert_escrow_is(&f, CONSULT_PRICE);

        // A standard token again settles normally.
        f.token.set_mode(&EXACT);
        f.voucher.settle(&id);
        assert_escrow_is(&f, 0);
    }
}

#[test]
fn short_delivering_token_cannot_refund() {
    // With a single voucher the escrow holds exactly its amount, so a token
    // that debits more than it sends cannot even complete the transfer: the
    // token refuses it (`TokenTransferFailed`). Either way it fails closed.
    for (mode, expected) in [
        (SKIM_RECIPIENT, VoucherError::TokenAmountMismatch),
        (OVERDRAW_SENDER, VoucherError::TokenTransferFailed),
        (NO_OP, VoucherError::TokenAmountMismatch),
    ] {
        let f = setup_mock_token();
        let id = try_fund(&f).unwrap();
        advance(&f.env, 31 * 24 * 60 * 60);
        let funder_before = f.token.balance(&f.funder);

        f.token.set_mode(&mode);
        assert_eq!(err_of(f.voucher.try_refund(&id)), expected, "mode {mode}");
        assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Funded);
        assert_eq!(f.token.balance(&f.funder), funder_before);
        assert_escrow_is(&f, CONSULT_PRICE);
    }
}

#[test]
fn overdrawing_payout_cannot_eat_another_vouchers_escrow() {
    // Two vouchers share one escrow balance. A token that debits more than it
    // sends would let settling the first quietly take from the second.
    let f = setup_mock_token();
    let first = fund_attested(&f);
    let _second = try_fund(&f).unwrap();
    advance(&f.env, DISPUTE_WINDOW + 1);

    f.token.set_mode(&OVERDRAW_SENDER);
    assert_eq!(
        err_of(f.voucher.try_settle(&first)),
        VoucherError::TokenAmountMismatch
    );
    assert_escrow_is(&f, 2 * CONSULT_PRICE);
}

#[test]
fn short_delivering_token_cannot_reduce_amount() {
    let f = setup_mock_token();
    let id = f.voucher.create_voucher(
        &f.funder,
        &patient(&f.env),
        &f.provider,
        &CONSULT,
        &(2 * CONSULT_PRICE),
        &EXPIRES_AT,
    );
    f.token.set_mode(&SKIM_RECIPIENT);
    assert_eq!(
        err_of(f.voucher.try_reduce_amount(&f.funder, &id, &CONSULT_PRICE)),
        VoucherError::TokenAmountMismatch
    );
    assert_eq!(f.voucher.get_voucher(&id).amount, 2 * CONSULT_PRICE);
    assert_escrow_is(&f, 2 * CONSULT_PRICE);
}
