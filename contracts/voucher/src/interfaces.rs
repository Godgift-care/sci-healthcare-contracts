use soroban_sdk::{contractclient, Address, BytesN, Env};

/// The slice of `sci-registry` this contract depends on.
///
/// Declared locally, over primitives only, so the two contracts share no
/// types and can be upgraded independently.
#[allow(dead_code)]
#[contractclient(name = "RegistryClient")]
pub trait RegistryInterface {
    fn is_active_provider(env: Env, provider_addr: Address) -> bool;
    fn is_attester(env: Env, addr: Address) -> bool;
    fn get_service_price(env: Env, provider_addr: Address, code: u32) -> i128;
    fn is_service_available(env: Env, provider_addr: Address, code: u32) -> bool;
}

/// The slice of `sci-receipt` this contract depends on.
#[allow(dead_code, clippy::too_many_arguments)]
#[contractclient(name = "ReceiptClient")]
pub trait ReceiptInterface {
    fn mint(
        env: Env,
        minter: Address,
        voucher_id: u64,
        beneficiary_ref: BytesN<32>,
        provider: Address,
        service_code: u32,
        amount: i128,
    );
}
