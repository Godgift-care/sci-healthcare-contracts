#![no_std]

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
mod test;

use soroban_sdk::{contract, contractimpl, token, Address, BytesN, Env};

#[allow(unused_imports)]
pub use events::{
    AdminChanged, AdminProposed, DisputeResolved, Initialized, VoucherAttested, VoucherClaimed,
    VoucherCreated, VoucherDisputed, VoucherRefunded, VoucherSettled,
};
pub use interfaces::{ReceiptClient, RegistryClient};
pub use types::{Config, DataKey, Voucher, VoucherError, VoucherStatus};

use storage::{
    bump_next_id, clear_pending_admin, extend_instance, has_config, peek_next_id, read_config,
    read_pending_admin, read_voucher, write_config, write_pending_admin, write_voucher,
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
        fee_account: Address,
    ) -> Result<(), VoucherError> {
        if has_config(&env) {
            return Err(VoucherError::AlreadyInitialized);
        }
        if fee_bps > MAX_FEE_BPS {
            return Err(VoucherError::InvalidFee);
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

        // Escrow first, record second: if the transfer fails the whole
        // invocation reverts and no voucher is written.
        let escrow = env.current_contract_address();
        let token_client = token::Client::new(&env, &config.token);
        token_client.transfer(&funder, &escrow, &amount);

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

    /// Provider marks the patient as presented and the service as begun.
    ///
    /// A provider suspended after the voucher was funded cannot claim it;
    /// the voucher simply runs to expiry and refunds.
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
        }
        .publish(&env);
        Ok(())
    }

    /// Attests that the service was delivered, opening the dispute window.
    ///
    /// Only a registry attester may call this — deliberately *not* the
    /// provider being paid. Separating the party who gets paid from the
    /// party who confirms delivery is the main check in this protocol.
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
        voucher.status = VoucherStatus::Attested;
        voucher.attested_at = now;
        voucher.dispute_deadline = now
            .checked_add(config.dispute_window)
            .ok_or(VoucherError::MathOverflow)?;
        write_voucher(&env, &voucher);

        VoucherAttested {
            attester,
            voucher_id,
            attested_at: now,
            dispute_deadline: voucher.dispute_deadline,
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
    /// - A `Funded` voucher is refundable once it expires unclaimed.
    /// - A `Claimed` voucher that was never attested is refundable once
    ///   `CLAIM_GRACE_SECS` have passed after expiry, so a claim alone can
    ///   never lock the funder's money forever.
    ///
    /// Permissionless for the same reason as `settle`.
    pub fn refund(env: Env, voucher_id: u64) -> Result<(), VoucherError> {
        let config = read_config(&env)?;
        let mut voucher = read_voucher(&env, voucher_id)?;

        let refundable_at = Self::refundable_at_for(&voucher)?;
        if env.ledger().timestamp() < refundable_at {
            return Err(VoucherError::NotYetExpired);
        }

        Self::pay_back(&env, &config, &mut voucher);
        Ok(())
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
        if config.admin != admin {
            return Err(VoucherError::NotAuthorized);
        }
        admin.require_auth();

        let mut voucher = read_voucher(&env, voucher_id)?;
        if voucher.status != VoucherStatus::Disputed {
            return Err(VoucherError::InvalidStatus);
        }

        DisputeResolved {
            admin,
            voucher_id,
            refunded_funder: refund_funder,
        }
        .publish(&env);

        if refund_funder {
            Self::pay_back(&env, &config, &mut voucher);
        } else {
            Self::pay_out(&env, &config, &mut voucher)?;
        }
        Ok(())
    }

    // ----- administration -----

    /// Nominates a new admin. Takes effect only once they accept.
    ///
    /// Two steps so a typo in the new address cannot permanently strand
    /// dispute resolution. Proposing again replaces the earlier nominee.
    pub fn propose_admin(env: Env, admin: Address, new_admin: Address) -> Result<(), VoucherError> {
        let config = read_config(&env)?;
        if config.admin != admin {
            return Err(VoucherError::NotAuthorized);
        }
        admin.require_auth();

        write_pending_admin(&env, &new_admin);
        extend_instance(&env);
        AdminProposed {
            admin,
            pending_admin: new_admin,
        }
        .publish(&env);
        Ok(())
    }

    /// The nominated address accepts and becomes admin.
    pub fn accept_admin(env: Env, new_admin: Address) -> Result<(), VoucherError> {
        let pending = read_pending_admin(&env).ok_or(VoucherError::NoPendingAdmin)?;
        if pending != new_admin {
            return Err(VoucherError::NotAuthorized);
        }
        new_admin.require_auth();

        let mut config = read_config(&env)?;
        config.admin = new_admin.clone();
        write_config(&env, &config);
        clear_pending_admin(&env);
        extend_instance(&env);
        AdminChanged { new_admin }.publish(&env);
        Ok(())
    }

    // ----- views -----

    pub fn get_pending_admin(env: Env) -> Option<Address> {
        read_pending_admin(&env)
    }

    /// Earliest ledger time at which `refund` will succeed, or
    /// `InvalidStatus` if this voucher can no longer be refunded.
    pub fn refundable_at(env: Env, voucher_id: u64) -> Result<u64, VoucherError> {
        Self::refundable_at_for(&read_voucher(&env, voucher_id)?)
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

    // ----- internal -----

    fn refundable_at_for(voucher: &Voucher) -> Result<u64, VoucherError> {
        match voucher.status {
            VoucherStatus::Funded => Ok(voucher.expires_at),
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
        amount
            .checked_mul(fee_bps as i128)
            .ok_or(VoucherError::MathOverflow)?
            .checked_div(BPS_DENOMINATOR)
            .ok_or(VoucherError::MathOverflow)
    }

    /// Pays the provider, takes the fee, and mints the care receipt.
    fn pay_out(env: &Env, config: &Config, voucher: &mut Voucher) -> Result<(), VoucherError> {
        let fee = Self::fee_for(voucher.amount, config.fee_bps)?;
        let net = voucher.amount - fee;

        let token_client = token::Client::new(env, &config.token);
        let escrow = env.current_contract_address();

        token_client.transfer(&escrow, &voucher.provider, &net);
        if fee > 0 {
            token_client.transfer(&escrow, &config.fee_account, &fee);
        }

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
        }
        .publish(env);
        Ok(())
    }

    /// Returns the full escrowed amount to the funder. No fee is taken on
    /// care that never happened.
    fn pay_back(env: &Env, config: &Config, voucher: &mut Voucher) {
        token::Client::new(env, &config.token).transfer(
            &env.current_contract_address(),
            &voucher.funder,
            &voucher.amount,
        );

        voucher.status = VoucherStatus::Refunded;
        write_voucher(env, voucher);

        VoucherRefunded {
            funder: voucher.funder.clone(),
            voucher_id: voucher.id,
            amount: voucher.amount,
        }
        .publish(env);
    }
}
