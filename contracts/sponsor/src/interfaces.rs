//! The slices of the other contracts this contract depends on.
//!
//! Declared locally, over primitives only, so the contracts share no types
//! and can be upgraded independently.

use soroban_sdk::{contractclient, contracterror, Address, BytesN, Env, String};

/// The slice of `sci-registry` this contract depends on.
#[allow(dead_code)]
#[contractclient(name = "RegistryClient")]
pub trait RegistryInterface {
    /// The country a clinic is registered in, as an ISO 3166-1 alpha-2
    /// code. Empty when the clinic is not registered.
    fn get_provider_country(env: Env, provider_addr: Address) -> String;
}

/// The slice of `sci-voucher` this contract depends on.
#[allow(dead_code)]
#[contractclient(name = "VoucherClient")]
pub trait VoucherInterface {
    /// The fields of a voucher this contract needs to match it, in one
    /// call.
    ///
    /// `(beneficiary_ref, funder, provider, service_code, amount, status,
    /// created_at, expires_at)`, where `status` is the voucher's
    /// `VoucherStatus` as a number: 0 `Funded`, 1 `Claimed`, 2 `Attested`,
    /// 3 `Settled`, 4 `Disputed`, 5 `Refunded`. `None` when the voucher
    /// does not exist.
    #[allow(clippy::type_complexity)]
    fn voucher_summary(
        env: Env,
        voucher_id: u64,
    ) -> Option<(BytesN<32>, Address, Address, u32, i128, u32, u64, u64)>;

    /// Escrows `amount` for one service at one active provider and returns
    /// the new voucher's id.
    #[allow(clippy::too_many_arguments)]
    fn create_voucher(
        env: Env,
        funder: Address,
        beneficiary_ref: BytesN<32>,
        provider: Address,
        service_code: u32,
        amount: i128,
        expires_at: u64,
    ) -> Result<u64, VoucherError>;
}

/// The voucher contract's error codes, mirrored so this contract can read
/// the result of a cross-contract call without sharing types with it.
///
/// The voucher contract is a separate deployment with its own numbering.
/// Only the codes a sponsorship can run into are listed; anything else comes
/// back as `VoucherCallFailed`.
#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum VoucherError {
    /// `ProviderNotActive` in the voucher contract.
    ProviderNotActive = 5,
    /// `ServiceNotOffered` in the voucher contract.
    ServiceNotOffered = 6,
    /// `AmountBelowPrice` in the voucher contract.
    AmountBelowPrice = 7,
    /// `ServiceUnavailable` in the voucher contract.
    ServiceUnavailable = 18,
}
