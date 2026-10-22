#![no_std]
// A panic aborts the whole invocation with an opaque host error. Contract
// code must return a typed error instead, so both are denied outside tests.
// Tests keep them for brevity: they are fail-fast assertions, not contract
// behaviour.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

//! # SCI Healthcare — Care Vouchers
//!
//! Escrow for a single, purpose-bound commitment to pay one verified
//! provider for one service.
//!
//! ## Why purpose-bound
//!
//! This contract deliberately has no concept of a balance. Money enters
//! only as a voucher naming a specific provider, a specific service and an
//! expiry, and it can leave only to that provider (on attested delivery) or
//! back to the funder (on expiry or upheld dispute). There is no general
//! stored value and no float, which is what keeps the protocol on the
//! prepaid-voucher side of the line rather than the e-money side of it.
//!
//! ## Why non-custodial
//!
//! Funds move directly from the funder's own account under the funder's own
//! signature. No operator ever holds user balances.
//!
//! ## What is deliberately not here
//!
//! No protected health information. `service_code` is a coarse category and
//! `beneficiary_ref` is an opaque salted hash. No diagnosis, result or name
//! touches the ledger.
//!
//! ## Trust model — stated plainly
//!
//! This is trust-*minimised*, not trustless. Attestation that care actually
//! happened is an off-chain fact. The protocol narrows the room to cheat
//! with a separate attester role, a funder dispute window, and admin
//! resolution — it does not eliminate it.

mod events;
mod interfaces;
mod storage;
mod types;

#[cfg(test)]
mod economics_test;
#[cfg(test)]
mod test;

use sci_common as common;
use soroban_sdk::{contract, contractimpl, token, Address, BytesN, Env, String};

#[allow(unused_imports)]
pub use events::{
    AdminChanged, AdminProposed, AttestationCapChanged, DisputeResolved, Initialized,
    VoucherAmountReduced, VoucherAttested, VoucherClaimed, VoucherCreated, VoucherDisputed,
    VoucherRefunded, VoucherSettled,
};
pub use interfaces::{ReceiptClient, RegistryClient};
pub use types::{Config, DataKey, Voucher, VoucherError, VoucherStatus};

use storage::{
    bump_next_id, decrease_open_escrow, extend_instance, has_config, increase_open_escrow,
    peek_next_id, read_attestation_cap, read_attestations, read_config, read_open_escrow,
    read_voucher, write_attestation_cap, write_attestations, write_config, write_voucher,
};

const BPS_DENOMINATOR: i128 = 10_000;
/// Ceiling on the protocol fee, enforced at initialization. 10%.
const MAX_FEE_BPS: u32 = 1_000;
/// How long past expiry a claimed-but-never-attested voucher stays locked
/// before the funder can take the money back. Seven days.
///
/// Without this, a provider that claims a voucher and never gets an
/// attestation would hold the funder's money in escrow indefinitely unless
/// the funder noticed and disputed. The grace period gives an attester
/// that is merely slow time to confirm care that happened near expiry.
pub const CLAIM_GRACE_SECS: u64 = 7 * 24 * 60 * 60;
/// Attestations one attester may make per UTC day until the admin sets
/// another cap with `set_attestation_cap`.
///
/// Limits the blast radius of a stolen attester key: it can confirm at most
/// this many vouchers before the day ends, buying the admin time to remove
/// it from the registry.
pub const DEFAULT_MAX_ATTESTATIONS_PER_DAY: u32 = 100;
/// Seconds in a UTC day: buckets the per-attester attestation counter.
const SECONDS_PER_DAY: u64 = 24 * 60 * 60;

#[contract]
pub struct VoucherEscrow;

#[allow(clippy::too_many_arguments)]
#[contractimpl]
impl VoucherEscrow {
    /// Fixes protocol configuration. Callable exactly once.
    #[allow(clippy::too_many_arguments)]
    pub fn initialize(
        env: Env,
        admin: Address,
        registry: Address,
        receipt_book: Address,
        token: Address,
        dispute_window: u64,
        fee_bps: u32,
        attester_share_bps: u32,
        fee_account: Address,
    ) -> Result<(), VoucherError> {
        if has_config(&env) {
            return Err(VoucherError::AlreadyInitialized);
        }
        if fee_bps > MAX_FEE_BPS {
            return Err(VoucherError::InvalidFee);
        }
        // The attester's share is carved out of the protocol fee, so it can
        // never be larger than the fee itself.
        if attester_share_bps > fee_bps {
            return Err(VoucherError::InvalidAttesterShare);
        }

        write_config(
            &env,
            &Config {
                admin: admin.clone(),
                registry,
                receipt_book,
                token,
                dispute_window,
                fee_bps,
                attester_share_bps,
                fee_account,
            },
        );
        extend_instance(&env);
        Initialized { admin }.publish(&env);
        Ok(())
    }

    /// Escrows `amount` against one service at one active provider.
    ///
    /// The funder need not be the patient. A relative sending from abroad
    /// funds a voucher exactly the same way, which is the point: this is a
    /// remittance that can only be spent on the care it was sent for.
    pub fn create_voucher(
        env: Env,
        funder: Address,
        beneficiary_ref: BytesN<32>,
        provider: Address,
        service_code: u32,
        amount: i128,
        expires_at: u64,
    ) -> Result<u64, VoucherError> {
        funder.require_auth();

        if amount <= 0 {
            return Err(VoucherError::InvalidAmount);
        }
        let now = env.ledger().timestamp();
        if expires_at <= now {
            return Err(VoucherError::InvalidExpiry);
        }

        let config = read_config(&env)?;
        let registry = RegistryClient::new(&env, &config.registry);

        if !registry.is_active_provider(&provider) {
            return Err(VoucherError::ProviderNotActive);
        }

        // Zero means the provider does not offer this service, or has
        // deactivated it.
        let price = registry.get_service_price(&provider, &service_code);
        if price == 0 {
            return Err(VoucherError::ServiceNotOffered);
        }
        if amount < price {
            return Err(VoucherError::AmountBelowPrice);
        }
        // Listed, but paused by the clinic (e.g. out of test kits).
        if !registry.is_service_available(&provider, &service_code) {
            return Err(VoucherError::ServiceUnavailable);
        }

        // Escrow first, record second: if the transfer fails the whole
        // invocation reverts and no voucher is written.
        let escrow = env.current_contract_address();
        let token_client = token::Client::new(&env, &config.token);
        token_client.transfer(&funder, &escrow, &amount);
        increase_open_escrow(&env, amount)?;

        let id = bump_next_id(&env);
        let voucher = Voucher {
            id,
            funder: funder.clone(),
            beneficiary_ref: beneficiary_ref.clone(),
            provider: provider.clone(),
            service_code,
            amount,
            status: VoucherStatus::Funded,
            created_at: now,
            expires_at,
            claimed_at: 0,
            attested_at: 0,
            dispute_deadline: 0,
            attester: None,
        };
        write_voucher(&env, &voucher);

        VoucherCreated {
            funder,
            provider,
            beneficiary_ref,
            voucher_id: id,
            service_code,
            amount,
            created_at: now,
            expires_at,
        }
        .publish(&env);
        Ok(id)
    }

    /// Gives back an overpayment without giving up the voucher.
    ///
    /// A funder who paid more than the listed price can bring the
    /// escrowed amount down to the service price and keep the voucher.
    /// The difference goes straight back to the funder, and no fee is
    /// taken on money that never became care.
    ///
    /// The floor is the price the registry lists **at call time**, so a
    /// clinic that has raised its price cannot have a funded voucher
    /// reduced below that price, and a clinic that has lowered it frees
    /// the funder to reduce further. Only a `Funded` voucher can be
    /// reduced: once the clinic has claimed it, the escrowed amount is
    /// what the clinic is expecting to be paid.
    ///
    /// Errors:
    ///
    /// - `NotAuthorized` — the caller is not the voucher's funder.
    /// - `InvalidStatus` — the voucher is no longer `Funded`.
    /// - `InvalidAmount` — `new_amount` is not positive.
    /// - `AmountNotReduced` — `new_amount` is not below the escrowed amount.
    /// - `ServiceNotOffered` — the service is no longer listed, so there is
    ///   no price to reduce towards.
    /// - `AmountBelowPrice` — `new_amount` is below the current listed price.
    pub fn reduce_amount(
        env: Env,
        funder: Address,
        voucher_id: u64,
        new_amount: i128,
    ) -> Result<(), VoucherError> {
        funder.require_auth();

        let mut voucher = read_voucher(&env, voucher_id)?;
        if voucher.funder != funder {
            return Err(VoucherError::NotAuthorized);
        }
        if voucher.status != VoucherStatus::Funded {
            return Err(VoucherError::InvalidStatus);
        }
        if new_amount <= 0 {
            return Err(VoucherError::InvalidAmount);
        }
        if new_amount >= voucher.amount {
            return Err(VoucherError::AmountNotReduced);
        }

        let config = read_config(&env)?;
        // The price at call time is the floor: whatever is left in escrow
        // must still cover the service.
        let price = RegistryClient::new(&env, &config.registry)
            .get_service_price(&voucher.provider, &voucher.service_code);
        if price == 0 {
            // The service is no longer listed, so there is no price to
            // reduce towards. The funder can still refund the voucher at
            // expiry.
            return Err(VoucherError::ServiceNotOffered);
        }
        if new_amount < price {
            return Err(VoucherError::AmountBelowPrice);
        }

        let difference = voucher.amount - new_amount;
        // Escrow first, record second: if the transfer fails the whole
        // invocation reverts and the escrowed amount is unchanged.
        let token_client = token::Client::new(&env, &config.token);
        Self::ensure_escrow_covers(&env, &token_client, voucher.amount)?;
        Self::send(&env, &token_client, &funder, difference)?;
        // The difference has left escrow, so the running total follows it.
        decrease_open_escrow(&env, difference)?;

        let previous_amount = voucher.amount;
        voucher.amount = new_amount;
        write_voucher(&env, &voucher);

        VoucherAmountReduced {
            funder,
            voucher_id,
            service_code: voucher.service_code,
            previous_amount,
            amount: new_amount,
            refunded: difference,
        }
        .publish(&env);
        Ok(())
    }

    /// Provider marks the patient as presented and the service as begun.
    ///
    /// A provider suspended after the voucher was funded cannot claim it;
    /// the voucher can be refunded at once instead of waiting for expiry.
    pub fn claim(env: Env, provider: Address, voucher_id: u64) -> Result<(), VoucherError> {
        provider.require_auth();

        let mut voucher = read_voucher(&env, voucher_id)?;
        if voucher.provider != provider {
            return Err(VoucherError::NotAuthorized);
        }
        if voucher.status != VoucherStatus::Funded {
            return Err(VoucherError::InvalidStatus);
        }

        let config = read_config(&env)?;
        if !RegistryClient::new(&env, &config.registry).is_active_provider(&provider) {
            return Err(VoucherError::ProviderNotActive);
        }

        let now = env.ledger().timestamp();
        if now >= voucher.expires_at {
            return Err(VoucherError::VoucherExpired);
        }

        voucher.status = VoucherStatus::Claimed;
        voucher.claimed_at = now;
        write_voucher(&env, &voucher);

        VoucherClaimed {
            provider,
            voucher_id,
            claimed_at: now,
            service_code: voucher.service_code,
            amount: voucher.amount,
        }
        .publish(&env);
        Ok(())
    }

    /// Attests that the service was delivered, opening the dispute window.
    ///
    /// Only a registry attester may call this — deliberately *not* the
    /// provider being paid. Separating the party who gets paid from the
    /// party who confirms delivery is the main check in this protocol.
    ///
    /// Each attester may attest at most the attestation cap per UTC day
    /// (`AttestationCapReached` beyond it); the counter resets at midnight.
    pub fn attest(env: Env, attester: Address, voucher_id: u64) -> Result<(), VoucherError> {
        attester.require_auth();

        let config = read_config(&env)?;
        let registry = RegistryClient::new(&env, &config.registry);
        if !registry.is_attester(&attester) {
            return Err(VoucherError::NotAuthorized);
        }

        let mut voucher = read_voucher(&env, voucher_id)?;
        // A provider cannot sign off on its own payment even if it somehow
        // holds an attester role.
        if attester == voucher.provider {
            return Err(VoucherError::NotAuthorized);
        }
        if voucher.status != VoucherStatus::Claimed {
            return Err(VoucherError::InvalidStatus);
        }

        let now = env.ledger().timestamp();
        let day = now / SECONDS_PER_DAY;
        let cap = read_attestation_cap(&env);
        let attested_today = read_attestations(&env, &attester, day);
        if attested_today >= cap {
            return Err(VoucherError::AttestationCapReached);
        }
        let attested_today = attested_today + 1;
        write_attestations(&env, &attester, day, attested_today);

        voucher.status = VoucherStatus::Attested;
        voucher.attested_at = now;
        voucher.attester = Some(attester.clone());
        voucher.dispute_deadline = now
            .checked_add(config.dispute_window)
            .ok_or(VoucherError::MathOverflow)?;
        write_voucher(&env, &voucher);

        VoucherAttested {
            attester,
            voucher_id,
            attested_at: now,
            dispute_deadline: voucher.dispute_deadline,
            service_code: voucher.service_code,
            amount: voucher.amount,
            remaining_today: cap - attested_today,
        }
        .publish(&env);
        Ok(())
    }

    /// Funder contests the claim before the dispute window closes.
    pub fn dispute(
        env: Env,
        funder: Address,
        voucher_id: u64,
        reason_code: u32,
    ) -> Result<(), VoucherError> {
        funder.require_auth();

        let mut voucher = read_voucher(&env, voucher_id)?;
        if voucher.funder != funder {
            return Err(VoucherError::NotAuthorized);
        }

        let now = env.ledger().timestamp();
        match voucher.status {
            // Provider took the voucher but never delivered.
            VoucherStatus::Claimed => {}
            // Attested, but the funder says otherwise. Only until the
            // window closes.
            VoucherStatus::Attested => {
                if now >= voucher.dispute_deadline {
                    return Err(VoucherError::DisputeWindowClosed);
                }
            }
            _ => return Err(VoucherError::InvalidStatus),
        }

        voucher.status = VoucherStatus::Disputed;
        write_voucher(&env, &voucher);

        VoucherDisputed {
            funder,
            voucher_id,
            reason_code,
            service_code: voucher.service_code,
            amount: voucher.amount,
        }
        .publish(&env);
        Ok(())
    }

    /// Releases escrow to the provider once the dispute window has closed.
    ///
    /// Permissionless: anyone may trigger settlement of an eligible
    /// voucher, so a provider is never dependent on the funder or an
    /// operator to get paid.
    pub fn settle(env: Env, voucher_id: u64) -> Result<(), VoucherError> {
        let config = read_config(&env)?;
        let mut voucher = read_voucher(&env, voucher_id)?;

        if voucher.status != VoucherStatus::Attested {
            return Err(VoucherError::InvalidStatus);
        }
        if env.ledger().timestamp() < voucher.dispute_deadline {
            return Err(VoucherError::DisputeWindowOpen);
        }

        Self::pay_out(&env, &config, &mut voucher)?;
        Ok(())
    }

    /// Returns escrow to the funder when care can no longer happen.
    ///
    /// - A `Funded` voucher is refundable once it expires unclaimed, or
    ///   immediately if its provider is no longer active in the registry.
    ///   A clinic suspended after the voucher was funded cannot serve it,
    ///   so the funder should not have to wait for expiry.
    /// - A `Claimed` voucher that was never attested is refundable once
    ///   `CLAIM_GRACE_SECS` have passed after expiry, so a claim alone can
    ///   never lock the funder's money forever. Suspension does not shorten
    ///   this path: care may already have been given before the clinic was
    ///   suspended, and an attester may still confirm it.
    ///
    /// Permissionless for the same reason as `settle`.
    pub fn refund(env: Env, voucher_id: u64) -> Result<(), VoucherError> {
        let config = read_config(&env)?;
        let mut voucher = read_voucher(&env, voucher_id)?;

        let refundable_at = Self::refundable_at_for(&env, &config, &voucher)?;
        if env.ledger().timestamp() < refundable_at {
            return Err(VoucherError::NotYetExpired);
        }

        Self::pay_back(&env, &config, &mut voucher)
    }

    /// Admin resolves a dispute. `refund_funder` picks the winner.
    ///
    /// This is the protocol's honest weak point and it is not hidden:
    /// resolution is a trusted human decision. It is scoped as narrowly as
    /// possible — the admin can only route already-escrowed funds to one of
    /// the two parties, and can never divert them elsewhere.
    pub fn resolve_dispute(
        env: Env,
        admin: Address,
        voucher_id: u64,
        refund_funder: bool,
    ) -> Result<(), VoucherError> {
        let config = read_config(&env)?;
        common::require_admin(&config.admin, &admin)?;

        let mut voucher = read_voucher(&env, voucher_id)?;
        if voucher.status != VoucherStatus::Disputed {
            return Err(VoucherError::InvalidStatus);
        }

        let to_provider = if refund_funder { 0 } else { voucher.amount };
        let fee = Self::fee_for(to_provider, config.fee_bps)?;
        DisputeResolved {
            admin,
            voucher_id,
            refunded_funder: refund_funder,
            to_provider,
            fee,
            to_funder: voucher.amount - to_provider,
        }
        .publish(&env);

        if refund_funder {
            Self::pay_back(&env, &config, &mut voucher)
        } else {
            Self::pay_out(&env, &config, &mut voucher)
        }
    }

    /// Admin resolves a dispute by splitting the escrow between the parties.
    ///
    /// `to_provider` is the gross part of the voucher awarded to the clinic,
    /// from `0` to the voucher amount. The clinic receives `to_provider` minus
    /// the protocol fee on that part, the fee account receives the fee, and the
    /// funder receives the rest. The three amounts add up to the voucher amount
    /// exactly, whatever `to_provider` is.
    ///
    /// A split of `0` is a full refund and a split of the whole amount is a
    /// full payout; in between, the voucher ends `Settled` because some care
    /// was paid for. A care receipt is minted only when the clinic receives
    /// something, so no receipt exists for a split that refunds everything.
    ///
    /// Like `resolve_dispute`, this is a trusted admin decision, but it can
    /// still route escrow only to the clinic, the fee account and the funder.
    pub fn resolve_split(
        env: Env,
        admin: Address,
        voucher_id: u64,
        to_provider: i128,
    ) -> Result<(), VoucherError> {
        let config = read_config(&env)?;
        common::require_admin(&config.admin, &admin)?;

        let mut voucher = read_voucher(&env, voucher_id)?;
        if voucher.status != VoucherStatus::Disputed {
            return Err(VoucherError::InvalidStatus);
        }
        if to_provider < 0 || to_provider > voucher.amount {
            return Err(VoucherError::InvalidAmount);
        }

        let fee = Self::fee_for(to_provider, config.fee_bps)?;
        let to_funder = voucher.amount - to_provider;

        DisputeResolved {
            admin,
            voucher_id,
            refunded_funder: to_provider == 0,
            to_provider,
            fee,
            to_funder,
        }
        .publish(&env);

        Self::pay_split(&env, &config, &mut voucher, to_provider, fee, to_funder)?;
        Ok(())
    }

    // ----- administration -----

    /// Nominates a new admin. Takes effect only once they accept.
    ///
    /// Two steps so a typo in the new address cannot permanently strand
    /// dispute resolution. Proposing again replaces the earlier nominee.
    pub fn propose_admin(env: Env, admin: Address, new_admin: Address) -> Result<(), VoucherError> {
        let config = read_config(&env)?;
        common::require_admin(&config.admin, &admin)?;

        common::propose_admin(&env, &DataKey::PendingAdmin, &new_admin);
        AdminProposed {
            admin,
            pending_admin: new_admin,
        }
        .publish(&env);
        Ok(())
    }

    /// The nominated address accepts and becomes admin.
    pub fn accept_admin(env: Env, new_admin: Address) -> Result<(), VoucherError> {
        let new_admin = common::take_pending_admin(&env, &DataKey::PendingAdmin, &new_admin)?;

        let mut config = read_config(&env)?;
        config.admin = new_admin.clone();
        write_config(&env, &config);
        AdminChanged { new_admin }.publish(&env);
        Ok(())
    }

    /// Sets how many attestations one attester may make per UTC day.
    ///
    /// Takes effect at once, including for today's count. Must be at least
    /// 1: to stop an attester entirely, remove it from the registry.
    pub fn set_attestation_cap(env: Env, admin: Address, cap: u32) -> Result<(), VoucherError> {
        let config = read_config(&env)?;
        common::require_admin(&config.admin, &admin)?;
        if cap == 0 {
            return Err(VoucherError::InvalidAttestationCap);
        }

        write_attestation_cap(&env, cap);
        extend_instance(&env);
        AttestationCapChanged { admin, cap }.publish(&env);
        Ok(())
    }

    // ----- views -----

    /// Most attestations one attester may make per UTC day.
    pub fn get_attestation_cap(env: Env) -> u32 {
        read_attestation_cap(&env)
    }

    /// Attestations `attester` may still make today (UTC).
    pub fn attestations_left(env: Env, attester: Address) -> u32 {
        let day = env.ledger().timestamp() / SECONDS_PER_DAY;
        read_attestation_cap(&env).saturating_sub(read_attestations(&env, &attester, day))
    }

    pub fn get_pending_admin(env: Env) -> Option<Address> {
        common::read_pending_admin(&env, &DataKey::PendingAdmin)
    }

    /// Earliest ledger time at which `refund` will succeed, or
    /// `InvalidStatus` if this voucher can no longer be refunded.
    ///
    /// A funded voucher whose provider is no longer active returns the
    /// current ledger time: it is refundable now, before expiry.
    pub fn refundable_at(env: Env, voucher_id: u64) -> Result<u64, VoucherError> {
        let voucher = read_voucher(&env, voucher_id)?;
        let config = read_config(&env)?;
        Self::refundable_at_for(&env, &config, &voucher)
    }

    pub fn get_voucher(env: Env, voucher_id: u64) -> Result<Voucher, VoucherError> {
        read_voucher(&env, voucher_id)
    }

    pub fn get_config(env: Env) -> Result<Config, VoucherError> {
        read_config(&env)
    }

    pub fn next_voucher_id(env: Env) -> u64 {
        peek_next_id(&env)
    }

    /// Fee and net payout for an amount, without mutating anything.
    /// Lets a client show the provider exactly what will land.
    pub fn quote(env: Env, amount: i128) -> Result<(i128, i128), VoucherError> {
        let config = read_config(&env)?;
        let fee = Self::fee_for(amount, config.fee_bps)?;
        Ok((fee, amount - fee))
    }

    /// Read-only solvency check: `(open_escrow, token_balance)`.
    ///
    /// `open_escrow` is the running total of amounts belonging to vouchers
    /// that have been funded and not yet settled or refunded. `token_balance`
    /// is the contract's actual balance of the settlement token. The two are
    /// equal when the accounting is correct; anything else is a bug or tokens
    /// sent to the contract outside a voucher.
    ///
    /// No authorisation: anyone may run the check.
    pub fn escrow_summary(env: Env) -> Result<(i128, i128), VoucherError> {
        let config = read_config(&env)?;
        let open_escrow = read_open_escrow(&env);
        let token_balance =
            token::Client::new(&env, &config.token).balance(&env.current_contract_address());
        Ok((open_escrow, token_balance))
    }

    /// Crate version this contract was built from, e.g. `"0.2.0"`.
    ///
    /// Lets integrators and the indexer tell deployments apart without
    /// comparing wasm hashes.
    pub fn version(env: Env) -> String {
        String::from_str(&env, env!("CARGO_PKG_VERSION"))
    }

    // ----- internal -----

    fn refundable_at_for(
        env: &Env,
        config: &Config,
        voucher: &Voucher,
    ) -> Result<u64, VoucherError> {
        match voucher.status {
            VoucherStatus::Funded => {
                let now = env.ledger().timestamp();
                // Keep the ordinary expiry path free of a cross-contract
                // call, so a registry problem can never block a refund that
                // was already due. Only a voucher still inside its validity
                // needs the registry to decide whether the clinic can serve
                // it at all.
                if now >= voucher.expires_at {
                    Ok(voucher.expires_at)
                } else if !RegistryClient::new(env, &config.registry)
                    .is_active_provider(&voucher.provider)
                {
                    Ok(now)
                } else {
                    Ok(voucher.expires_at)
                }
            }
            VoucherStatus::Claimed => voucher
                .expires_at
                .checked_add(CLAIM_GRACE_SECS)
                .ok_or(VoucherError::MathOverflow),
            _ => Err(VoucherError::InvalidStatus),
        }
    }

    /// Basis-point fee. Integer math throughout; truncation favours the
    /// provider, never the protocol.
    fn fee_for(amount: i128, fee_bps: u32) -> Result<i128, VoucherError> {
        Self::bps_of(amount, fee_bps)
    }

    /// Share of `amount` in basis points, rounded down.
    fn bps_of(amount: i128, bps: u32) -> Result<i128, VoucherError> {
        amount
            .checked_mul(bps as i128)
            .ok_or(VoucherError::MathOverflow)?
            .checked_div(BPS_DENOMINATOR)
            .ok_or(VoucherError::MathOverflow)
    }

    /// Fails with `EscrowShortfall` when the contract holds less of the token
    /// than `amount`.
    ///
    /// Escrow is one token balance shared by every open voucher, so this can
    /// only happen if the issuer clawed funds back from the contract. Checking
    /// first turns what would be a host panic deep inside the token into a
    /// defined error, and the whole invocation reverts: nothing is half-paid.
    fn ensure_escrow_covers(
        env: &Env,
        token_client: &token::Client,
        amount: i128,
    ) -> Result<(), VoucherError> {
        if token_client.balance(&env.current_contract_address()) < amount {
            return Err(VoucherError::EscrowShortfall);
        }
        Ok(())
    }

    /// Moves `amount` out of escrow, turning a refused transfer — a frozen
    /// (deauthorized) escrow or recipient — into `TokenTransferFailed`.
    fn send(
        env: &Env,
        token_client: &token::Client,
        to: &Address,
        amount: i128,
    ) -> Result<(), VoucherError> {
        match token_client.try_transfer(&env.current_contract_address(), to, &amount) {
            Ok(Ok(())) => Ok(()),
            _ => Err(VoucherError::TokenTransferFailed),
        }
    }

    /// Pays the provider, splits the fee between the attester and the fee
    /// account, and mints the care receipt.
    ///
    /// `Config` guarantees `attester_share_bps <= fee_bps`, so the attester
    /// share can never exceed the fee and the three payouts always sum to the
    /// voucher amount exactly:
    ///
    /// ```text
    /// net             = amount - fee
    /// attester_share  = amount * attester_share_bps / 10_000
    /// fee_account     = fee - attester_share
    /// ```
    ///
    /// A voucher attested before attesters were stored has no attester to pay,
    /// so the whole fee goes to the fee account, as it did before.
    fn pay_out(env: &Env, config: &Config, voucher: &mut Voucher) -> Result<(), VoucherError> {
        let fee = Self::fee_for(voucher.amount, config.fee_bps)?;
        let net = voucher.amount - fee;
        let attester_share = match &voucher.attester {
            Some(_) => Self::bps_of(voucher.amount, config.attester_share_bps)?,
            None => 0,
        };
        let fee_account_share = fee - attester_share;

        let token_client = token::Client::new(env, &config.token);
        let escrow = env.current_contract_address();

        Self::ensure_escrow_covers(env, &token_client, voucher.amount)?;
        Self::send(env, &token_client, &voucher.provider, net)?;
        if let Some(attester) = &voucher.attester {
            if attester_share > 0 {
                Self::send(env, &token_client, attester, attester_share)?;
            }
        }
        if fee_account_share > 0 {
            Self::send(env, &token_client, &config.fee_account, fee_account_share)?;
        }
        decrease_open_escrow(env, voucher.amount)?;

        voucher.status = VoucherStatus::Settled;
        write_voucher(env, voucher);

        ReceiptClient::new(env, &config.receipt_book).mint(
            &escrow,
            &voucher.id,
            &voucher.beneficiary_ref,
            &voucher.provider,
            &voucher.service_code,
            &voucher.amount,
        );

        VoucherSettled {
            provider: voucher.provider.clone(),
            voucher_id: voucher.id,
            net,
            fee,
            attester_share,
            service_code: voucher.service_code,
            amount: voucher.amount,
        }
        .publish(env);
        Ok(())
    }

    /// Pays a resolved dispute in parts: `to_provider` gross to the clinic
    /// (minus its fee), the fee to the fee account and `to_funder` back to the
    /// funder. The caller has already checked that the parts add up.
    fn pay_split(
        env: &Env,
        config: &Config,
        voucher: &mut Voucher,
        to_provider: i128,
        fee: i128,
        to_funder: i128,
    ) -> Result<(), VoucherError> {
        let net = to_provider - fee;
        let token_client = token::Client::new(env, &config.token);
        let escrow = env.current_contract_address();

        Self::ensure_escrow_covers(env, &token_client, voucher.amount)?;
        if net > 0 {
            Self::send(env, &token_client, &voucher.provider, net)?;
        }
        if fee > 0 {
            Self::send(env, &token_client, &config.fee_account, fee)?;
        }
        if to_funder > 0 {
            Self::send(env, &token_client, &voucher.funder, to_funder)?;
        }

        voucher.status = if to_provider == 0 {
            VoucherStatus::Refunded
        } else {
            VoucherStatus::Settled
        };
        write_voucher(env, voucher);

        // No care was paid for when the split sends everything back, so no
        // receipt is minted and the beneficiary's episode count stays put.
        if to_provider > 0 {
            ReceiptClient::new(env, &config.receipt_book).mint(
                &escrow,
                &voucher.id,
                &voucher.beneficiary_ref,
                &voucher.provider,
                &voucher.service_code,
                &to_provider,
            );
        }
        Ok(())
    }

    /// Returns the full escrowed amount to the funder. No fee is taken on
    /// care that never happened.
    fn pay_back(env: &Env, config: &Config, voucher: &mut Voucher) -> Result<(), VoucherError> {
        let token_client = token::Client::new(env, &config.token);
        Self::ensure_escrow_covers(env, &token_client, voucher.amount)?;
        Self::send(env, &token_client, &voucher.funder, voucher.amount)?;
        decrease_open_escrow(env, voucher.amount)?;

        voucher.status = VoucherStatus::Refunded;
        write_voucher(env, voucher);

        VoucherRefunded {
            funder: voucher.funder.clone(),
            voucher_id: voucher.id,
            amount: voucher.amount,
            service_code: voucher.service_code,
        }
        .publish(env);
        Ok(())
    }
}
