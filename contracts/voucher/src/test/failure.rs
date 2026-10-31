//! Ordering and failure handling around settlement.
//!
//! `settle`/`resolve_dispute` call the token twice and the receipt book once.
//! These tests use a token that refuses transfers, a token that reports
//! success without moving funds, and a receipt book that refuses mints, and
//! assert that a failed settlement leaves both the voucher state and every
//! balance exactly where they were.

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error,
    testutils::{Address as _, Ledger as _},
    token, Address, BytesN, Env, String,
};

use crate::{VoucherError, VoucherEscrow, VoucherEscrowClient, VoucherStatus};
use sci_receipt::{ReceiptBook, ReceiptBookClient};
use sci_registry::{ProviderStatus, Registry, RegistryClient};

const DISPUTE_WINDOW: u64 = 72 * 60 * 60;
const FEE_BPS: u32 = 100;
const START_TIME: u64 = 1_700_000_000;
const CONSULT: u32 = 101;
const CONSULT_PRICE: i128 = 30_000_000;
const ONE_DAY: u64 = 24 * 60 * 60;

// ----- a settlement token we can make misbehave -----

#[contracttype]
enum TokenKey {
    Balance(Address),
    RefuseTransfers,
    SilentTransfers,
}

#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
enum MockTokenError {
    Refused = 1,
    InsufficientBalance = 2,
}

/// A minimal SEP-41 token with two failure modes:
///
/// - `RefuseTransfers`: `transfer` returns a contract error, the way a real
///   token rejects a transfer it cannot honour.
/// - `SilentTransfers`: `transfer` returns success and moves nothing, the way
///   a non-compliant token could.
#[contract]
pub struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn mint(env: Env, to: Address, amount: i128) {
        let key = TokenKey::Balance(to);
        let balance: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage().persistent().set(&key, &(balance + amount));
    }

    pub fn set_refuse_transfers(env: Env, refuse: bool) {
        env.storage()
            .instance()
            .set(&TokenKey::RefuseTransfers, &refuse);
    }

    pub fn set_silent_transfers(env: Env, silent: bool) {
        env.storage()
            .instance()
            .set(&TokenKey::SilentTransfers, &silent);
    }

    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&TokenKey::Balance(id))
            .unwrap_or(0)
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();

        let refuse: bool = env
            .storage()
            .instance()
            .get(&TokenKey::RefuseTransfers)
            .unwrap_or(false);
        if refuse {
            panic_with_error!(&env, MockTokenError::Refused);
        }

        let silent: bool = env
            .storage()
            .instance()
            .get(&TokenKey::SilentTransfers)
            .unwrap_or(false);
        if silent {
            return;
        }

        let from_key = TokenKey::Balance(from);
        let from_balance: i128 = env.storage().persistent().get(&from_key).unwrap_or(0);
        if from_balance < amount {
            panic_with_error!(&env, MockTokenError::InsufficientBalance);
        }
        env.storage()
            .persistent()
            .set(&from_key, &(from_balance - amount));

        let to_key = TokenKey::Balance(to);
        let to_balance: i128 = env.storage().persistent().get(&to_key).unwrap_or(0);
        env.storage()
            .persistent()
            .set(&to_key, &(to_balance + amount));
    }
}

// ----- a receipt book that can reject mints -----

#[contracttype]
enum ReceiptKey {
    AcceptMints,
}

#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum MockReceiptError {
    MintRejected = 1,
}

/// Mirrors `sci-receipt` refusing a mint: the call returns an error instead of
/// recording a receipt. Mints are rejected until `set_accept_mints(true)`.
#[contract]
pub struct MockReceiptBook;

#[contractimpl]
impl MockReceiptBook {
    pub fn set_accept_mints(env: Env, accept: bool) {
        env.storage()
            .instance()
            .set(&ReceiptKey::AcceptMints, &accept);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn mint(
        env: Env,
        _minter: Address,
        _voucher_id: u64,
        _beneficiary_ref: BytesN<32>,
        _provider: Address,
        _service_code: u32,
        _amount: i128,
    ) -> Result<(), MockReceiptError> {
        let accept: bool = env
            .storage()
            .instance()
            .get(&ReceiptKey::AcceptMints)
            .unwrap_or(false);
        if accept {
            Ok(())
        } else {
            Err(MockReceiptError::MintRejected)
        }
    }
}

// ----- fixture -----

#[derive(Clone, Copy)]
enum TokenChoice {
    Mock,
    Sac,
}

#[derive(Clone, Copy)]
enum ReceiptChoice {
    Real,
    Rejecting,
}

struct Fixture {
    env: Env,
    voucher: VoucherEscrowClient<'static>,
    token: token::Client<'static>,
    token_addr: Address,
    receipt_addr: Address,
    escrow: Address,
    funder: Address,
    provider: Address,
    attester: Address,
    fee_account: Address,
}

impl Fixture {
    fn mock_token(&self) -> MockTokenClient<'static> {
        MockTokenClient::new(&self.env, &self.token_addr)
    }

    fn mock_receipts(&self) -> MockReceiptBookClient<'static> {
        MockReceiptBookClient::new(&self.env, &self.receipt_addr)
    }
}

fn setup(token_choice: TokenChoice, receipt_choice: ReceiptChoice) -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = START_TIME);

    let admin = Address::generate(&env);
    let funder = Address::generate(&env);
    let provider = Address::generate(&env);
    let attester = Address::generate(&env);
    let fee_account = Address::generate(&env);

    let token_addr = match token_choice {
        TokenChoice::Mock => env.register(MockToken, ()),
        TokenChoice::Sac => env
            .register_stellar_asset_contract_v2(Address::generate(&env))
            .address(),
    };
    let token_client = token::Client::new(&env, &token_addr);
    match token_choice {
        TokenChoice::Mock => {
            MockTokenClient::new(&env, &token_addr).mint(&funder, &1_000_000_000i128)
        }
        TokenChoice::Sac => {
            token::StellarAssetClient::new(&env, &token_addr).mint(&funder, &1_000_000_000i128)
        }
    }

    let registry_addr = env.register(Registry, ());
    let registry = RegistryClient::new(&env, &registry_addr);
    registry.initialize(&admin);

    // The voucher contract exists first so the receipt book can name it as
    // minter at initialisation, the order scripts/deploy.sh uses.
    let escrow = env.register(VoucherEscrow, ());
    let voucher = VoucherEscrowClient::new(&env, &escrow);
    let receipt_addr = match receipt_choice {
        ReceiptChoice::Real => {
            let addr = env.register(ReceiptBook, ());
            ReceiptBookClient::new(&env, &addr).initialize(&admin, &escrow);
            addr
        }
        ReceiptChoice::Rejecting => env.register(MockReceiptBook, ()),
    };
    voucher.initialize(
        &admin,
        &registry_addr,
        &receipt_addr,
        &token_addr,
        &DISPUTE_WINDOW,
        &FEE_BPS,
        &0u32,
        &fee_account,
    );

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
        token: token_client,
        token_addr,
        receipt_addr,
        escrow,
        funder,
        provider,
        attester,
        fee_account,
    }
}

fn patient(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[7u8; 32])
}

fn fund(f: &Fixture) -> u64 {
    f.voucher.create_voucher(
        &f.funder,
        &patient(&f.env),
        &f.provider,
        &CONSULT,
        &CONSULT_PRICE,
        &(START_TIME + 30 * ONE_DAY),
    )
}

/// Fund, claim and attest, then move past the dispute window so `settle` is
/// eligible.
fn attested(f: &Fixture) -> u64 {
    let id = fund(f);
    f.voucher.claim(&f.provider, &id);
    f.voucher.attest(&f.attester, &id);
    f.env
        .ledger()
        .with_mut(|li| li.timestamp = START_TIME + DISPUTE_WINDOW + 1);
    id
}

fn fee_for(amount: i128) -> i128 {
    amount * FEE_BPS as i128 / 10_000
}

// ----- a compliant token still settles -----

#[test]
fn settle_pays_out_with_a_compliant_mock_token() {
    // The escrow-balance check must not reject a token that behaves.
    let f = setup(TokenChoice::Mock, ReceiptChoice::Real);
    let id = attested(&f);

    f.voucher.settle(&id);

    let fee = fee_for(CONSULT_PRICE);
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Settled);
    assert_eq!(f.token.balance(&f.provider), CONSULT_PRICE - fee);
    assert_eq!(f.token.balance(&f.fee_account), fee);
    assert_eq!(f.token.balance(&f.escrow), 0);
}

// ----- failing token -----

#[test]
fn failing_token_leaves_voucher_and_balances_unchanged() {
    let f = setup(TokenChoice::Mock, ReceiptChoice::Real);
    let id = attested(&f);
    f.mock_token().set_refuse_transfers(&true);

    let err = f.voucher.try_settle(&id).err().unwrap().unwrap();
    assert_eq!(err, VoucherError::TokenTransferFailed);

    // Nothing paid, nothing marked paid.
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Attested);
    assert_eq!(f.token.balance(&f.escrow), CONSULT_PRICE);
    assert_eq!(f.token.balance(&f.provider), 0);
    assert_eq!(f.token.balance(&f.fee_account), 0);
}

#[test]
fn failing_token_leaves_refund_unchanged() {
    let f = setup(TokenChoice::Mock, ReceiptChoice::Real);
    let before = f.token.balance(&f.funder);
    let id = fund(&f);
    f.env
        .ledger()
        .with_mut(|li| li.timestamp = START_TIME + 31 * ONE_DAY);
    f.mock_token().set_refuse_transfers(&true);

    let direct = f.token.try_transfer(&f.escrow, &f.funder, &1i128);
    assert!(direct.is_err());
    let err = f.voucher.try_refund(&id).err().unwrap().unwrap();
    assert_eq!(err, VoucherError::TokenTransferFailed);

    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Funded);
    assert_eq!(f.token.balance(&f.funder), before - CONSULT_PRICE);
    assert_eq!(f.token.balance(&f.escrow), CONSULT_PRICE);
}

#[test]
fn failing_token_leaves_disputed_voucher_disputed() {
    let f = setup(TokenChoice::Mock, ReceiptChoice::Real);
    let id = fund(&f);
    f.voucher.claim(&f.provider, &id);
    f.voucher.dispute(&f.funder, &id, &1u32);
    let funder_after_funding = f.token.balance(&f.funder);
    f.mock_token().set_refuse_transfers(&true);

    let admin = f.voucher.get_config().admin;
    let err = f
        .voucher
        .try_resolve_dispute(&admin, &id, &true)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, VoucherError::TokenTransferFailed);

    // The dispute is still open, not silently resolved.
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Disputed);
    assert_eq!(f.token.balance(&f.funder), funder_after_funding);
    assert_eq!(f.token.balance(&f.escrow), CONSULT_PRICE);
}

// ----- token that reports success but moves nothing -----

#[test]
fn silent_token_cannot_mark_voucher_paid() {
    let f = setup(TokenChoice::Mock, ReceiptChoice::Real);
    let id = attested(&f);
    f.mock_token().set_silent_transfers(&true);

    let err = f.voucher.try_settle(&id).err().unwrap().unwrap();
    assert_eq!(err, VoucherError::TokenTransferFailed);

    // The token said yes and moved nothing: the voucher must not be Settled.
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Attested);
    assert_eq!(f.token.balance(&f.escrow), CONSULT_PRICE);
    assert_eq!(f.token.balance(&f.provider), 0);
    assert_eq!(f.token.balance(&f.fee_account), 0);
}

#[test]
fn silent_token_cannot_mark_refund_paid() {
    let f = setup(TokenChoice::Mock, ReceiptChoice::Real);
    let before = f.token.balance(&f.funder);
    let id = fund(&f);
    f.env
        .ledger()
        .with_mut(|li| li.timestamp = START_TIME + 31 * ONE_DAY);
    f.mock_token().set_silent_transfers(&true);

    // Silent by design: the transfer reports success and moves nothing.
    f.token.transfer(&f.escrow, &f.funder, &1i128);
    assert_eq!(f.token.balance(&f.funder), before - CONSULT_PRICE);

    let err = f.voucher.try_refund(&id).err().unwrap().unwrap();
    assert_eq!(err, VoucherError::TokenTransferFailed);

    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Funded);
    assert_eq!(f.token.balance(&f.funder), before - CONSULT_PRICE);
    assert_eq!(f.token.balance(&f.escrow), CONSULT_PRICE);
}

// ----- failing receipt book -----

#[test]
fn failing_receipt_book_rolls_back_payout() {
    let f = setup(TokenChoice::Sac, ReceiptChoice::Rejecting);
    let id = attested(&f);

    let err = f.voucher.try_settle(&id).err().unwrap().unwrap();
    assert_eq!(err, VoucherError::ReceiptMintFailed);

    // Money had already moved when the mint was rejected; the whole
    // invocation must be undone, balances included.
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Attested);
    assert_eq!(f.token.balance(&f.escrow), CONSULT_PRICE);
    assert_eq!(f.token.balance(&f.provider), 0);
    assert_eq!(f.token.balance(&f.fee_account), 0);
}

#[test]
fn failed_mint_does_not_strand_the_escrow() {
    // A receipt book outage must not lose the provider's money: once the book
    // accepts mints again, the same voucher settles and pays normally, and the
    // first attempt left no receipt behind.
    let f = setup(TokenChoice::Sac, ReceiptChoice::Rejecting);
    let id = attested(&f);
    assert!(f.voucher.try_settle(&id).is_err());

    f.mock_receipts().set_accept_mints(&true);
    f.voucher.settle(&id);

    let fee = fee_for(CONSULT_PRICE);
    assert_eq!(f.voucher.get_voucher(&id).status, VoucherStatus::Settled);
    assert_eq!(f.token.balance(&f.provider), CONSULT_PRICE - fee);
    assert_eq!(f.token.balance(&f.escrow), 0);
}
