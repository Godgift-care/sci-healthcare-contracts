#![no_std]
// A panic aborts the whole invocation with an opaque host error. Contract
// code must return a typed error instead, so both are denied outside tests.
// Tests keep them for brevity: they are fail-fast assertions, not contract
// behaviour.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

//! # SCI Healthcare — Sponsorship desk
//!
//! Lets a sponsor put a budget behind someone else's voucher. The sponsor
//! deposits a budget, sets the rules it wants, and the desk adds a *second*
//! voucher for the same care — funded by the sponsor — whenever a voucher
//! qualifies.
//!
//! ## Why a second voucher and not a bigger one
//!
//! The match is an ordinary voucher, created through the voucher contract
//! and escrowed there. Nothing about the care lifecycle is re-implemented
//! here: the clinic claims it, an attester confirms it, the funder can
//! dispute it, and the money is settled or refunded by the same code that
//! already handles every other voucher. This contract only decides *whether*
//! a match happens and *how much* it costs the sponsor.
//!
//! ## Why the money sits here
//!
//! The matching voucher is funded by this contract, so the sponsor's budget
//! is escrowed by the voucher contract exactly like a funder's own money.
//! The sponsor's budget is the difference between what it deposited and
//! what it has committed to matches; whatever is left can be withdrawn.
//!
//! ## What this contract is not
//!
//! It is not a source of care. A match never creates a claimable right on
//! its own: the money is only ever spent when the clinic delivers and an
//! attester confirms it, exactly as for any other voucher. The abuse cases
//! this design has to answer are written up in
//! the `sponsor` section of `docs/developers/contracts.md`.

mod events;
mod interfaces;
mod storage;
mod types;

#[cfg(test)]
mod test;

use sci_common as common;
use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    contract, contractimpl, token, vec, Address, BytesN, Env, IntoVal, MuxedAddress, String,
    Symbol, Vec,
};

pub use events::{
    AdminChanged, AdminProposed, BudgetDeposited, BudgetWithdrawn, Initialized, MatchCreated,
    MatchRefunded, MatchSettled, RulesSet,
};
pub use types::{
    Config, DataKey, MatchRecord, OpenMatch, Rules, SponsorAccount, SponsorError, Totals,
};

use interfaces::{RegistryClient, VoucherClient, VoucherError};
use storage::{
    available_of, extend_instance, has_config, read_account, read_config, read_match,
    read_original_of, read_totals, record_movement, write_account, write_config, write_match,
    write_original_of, write_totals,
};

/// Basis points in one hundred per cent.
pub const BPS_DENOMINATOR: i128 = 10_000;
/// Ceiling on the match ratio: twenty times the matched amount.
pub const MAX_MATCH_BPS: u32 = 200_000;
/// Most matches one sponsor may have open at once.
///
/// An open match is one whose matching voucher has neither settled nor been
/// refunded. The desk checks each of them when the sponsor's books are next
/// touched, so the list is kept short enough for that to stay cheap.
pub const MAX_OPEN_MATCHES: u32 = 20;
/// The voucher's `VoucherStatus` as a number.
const VOUCHER_STATUS_FUNDED: u32 = 0;
const VOUCHER_STATUS_SETTLED: u32 = 3;
const VOUCHER_STATUS_REFUNDED: u32 = 5;

#[contract]
pub struct SponsorDesk;

#[contractimpl]
impl SponsorDesk {
    /// Sets the addresses this contract works with and its admin. Callable
    /// exactly once.
    ///
    /// The voucher and registry contracts must already exist and be
    /// initialised: this contract reads prices and countries from the
    /// registry and creates vouchers in the voucher contract.
    pub fn initialize(
        env: Env,
        admin: Address,
        registry: Address,
        voucher: Address,
        token: Address,
    ) -> Result<(), SponsorError> {
        if has_config(&env) {
            return Err(SponsorError::AlreadyInitialized);
        }
        write_config(
            &env,
            &Config {
                admin: admin.clone(),
                registry,
                voucher,
                token,
                paused: false,
            },
        );
        extend_instance(&env);
        Initialized { admin }.publish(&env);
        Ok(())
    }

    /// Adds to a sponsor's budget.
    ///
    /// The money moves from the sponsor's own account under the sponsor's own
    /// signature, into this contract, which holds it on the sponsor's
    /// behalf until it is matched or withdrawn.
    pub fn deposit(env: Env, sponsor: Address, amount: i128) -> Result<(), SponsorError> {
        let config = Self::read_config(&env)?;
        if config.paused {
            return Err(SponsorError::Paused);
        }
        sponsor.require_auth();
        if amount <= 0 {
            return Err(SponsorError::InvalidAmount);
        }

        // The money moves first: if the transfer fails, nothing is recorded.
        let desk = env.current_contract_address();
        token::Client::new(&env, &config.token).transfer(&sponsor, &desk, &amount);

        let mut account = read_account(&env, &sponsor);
        let mut totals = read_totals(&env);
        record_movement(&mut account, &mut totals, amount, 0, 0, 0)?;
        write_account(&env, &account);
        write_totals(&env, &totals);

        BudgetDeposited {
            sponsor,
            amount,
            available: available_of(&account)?,
        }
        .publish(&env);
        Ok(())
    }

    /// Sets the rules the desk applies to this sponsor's matches.
    ///
    /// The rules are the sponsor's own. They can be changed at any time; a
    /// match already made keeps the rules it was made under.
    pub fn set_rules(env: Env, sponsor: Address, rules: Rules) -> Result<(), SponsorError> {
        Self::read_config(&env)?;
        sponsor.require_auth();
        Self::check_rules(&rules)?;

        let mut account = read_account(&env, &sponsor);
        account.rules = rules.clone();
        write_account(&env, &account);

        RulesSet {
            sponsor,
            match_bps: rules.match_bps,
            cap_per_voucher: rules.cap_per_voucher,
            min_voucher_amount: rules.min_voucher_amount,
            min_voucher_age_secs: rules.min_voucher_age_secs,
        }
        .publish(&env);
        Ok(())
    }

    /// Matches a voucher: the sponsor's budget pays for a second voucher
    /// for the same care.
    ///
    /// The matching voucher is created by the voucher contract and funded by
    /// this contract, so it is escrowed, claimed, attested and settled like
    /// any other. It carries the same beneficiary, the same clinic and the
    /// same service as the voucher that was matched, and it expires with it.
    ///
    /// The two are linked both ways: `get_match` takes the voucher that was
    /// matched, `get_match_of` takes the matching voucher.
    ///
    /// Errors:
    ///
    /// - `RulesNotSet` — the sponsor has not set any rules.
    /// - `VoucherNotFound` — there is no such voucher.
    /// - `VoucherNotFunded` — the voucher is not `Funded`.
    /// - `VoucherExpired` — the voucher has expired.
    /// - `AlreadyMatched` — the voucher has already been matched.
    /// - `SelfMatchNotAllowed` — the sponsor funded the voucher itself, or
    ///   this contract did.
    /// - `CountryNotEligible`, `ServiceNotEligible` — the clinic or the
    ///   service is outside the sponsor's rules.
    /// - `VoucherTooNew`, `VoucherTooSmall` — the voucher is outside the
    ///   sponsor's rules.
    /// - `MatchBelowPrice` — the match is worth less than the service, and
    ///   a voucher cannot be funded below the price of the care it pays for.
    /// - `BudgetExhausted` — the sponsor's budget cannot cover the match.
    pub fn match_voucher(env: Env, sponsor: Address, voucher_id: u64) -> Result<u64, SponsorError> {
        let config = Self::read_config(&env)?;
        if config.paused {
            return Err(SponsorError::Paused);
        }
        sponsor.require_auth();

        let mut account = read_account(&env, &sponsor);
        let mut totals = read_totals(&env);

        let rules = account.rules.clone();
        if rules.match_bps == 0 {
            return Err(SponsorError::RulesNotSet);
        }

        if read_match(&env, voucher_id).is_some() {
            return Err(SponsorError::AlreadyMatched);
        }

        let vouchers = VoucherClient::new(&env, &config.voucher);
        let (
            beneficiary_ref,
            funder,
            provider,
            service_code,
            amount,
            status,
            created_at,
            expires_at,
        ) = vouchers
            .voucher_summary(&voucher_id)
            .ok_or(SponsorError::VoucherNotFound)?;

        if status != VOUCHER_STATUS_FUNDED {
            return Err(SponsorError::VoucherNotFunded);
        }
        let now = env.ledger().timestamp();
        if expires_at <= now {
            return Err(SponsorError::VoucherExpired);
        }

        // The sponsor may not match a voucher it funded itself, and may not
        // match one this contract funded: either way the match would pay the
        // payer of the match.
        let desk = env.current_contract_address();
        if funder == sponsor || funder == desk {
            return Err(SponsorError::SelfMatchNotAllowed);
        }

        if now.saturating_sub(created_at) < rules.min_voucher_age_secs {
            return Err(SponsorError::VoucherTooNew);
        }
        if amount < rules.min_voucher_amount {
            return Err(SponsorError::VoucherTooSmall);
        }

        // The clinic has to be in one of the sponsor's countries, and the
        // service has to be one the sponsor matches. What the clinic is
        // allowed to bill for, and what the service costs, is the voucher
        // contract's business: it checks the price and the clinic's status
        // when it creates the matching voucher, and its answer comes back
        // as a typed error.
        if !rules.eligible_countries.is_empty() {
            let country =
                RegistryClient::new(&env, &config.registry).get_provider_country(&provider);
            if !rules.eligible_countries.contains(&country) {
                return Err(SponsorError::CountryNotEligible);
            }
        }
        if !rules.eligible_service_codes.is_empty()
            && !rules.eligible_service_codes.contains(service_code)
        {
            return Err(SponsorError::ServiceNotEligible);
        }

        let match_amount = Self::match_amount(amount, &rules)?;
        if match_amount <= 0 {
            return Err(SponsorError::MatchTooSmall);
        }

        let available = Self::budget_for(&env, &config, &mut account, &mut totals, match_amount)?;
        if available < match_amount {
            return Err(SponsorError::BudgetExhausted);
        }
        if account.open_matches.len() >= MAX_OPEN_MATCHES {
            return Err(SponsorError::TooManyOpenMatches);
        }

        // The matching voucher, funded by this contract and escrowed by the
        // voucher contract.
        let matched_id = Self::create_matching_voucher(
            &env,
            &config,
            &beneficiary_ref,
            &provider,
            service_code,
            match_amount,
            expires_at,
        )?;

        let record = MatchRecord {
            original_voucher_id: voucher_id,
            matched_voucher_id: matched_id,
            sponsor: sponsor.clone(),
            amount: match_amount,
            provider: provider.clone(),
            service_code,
            matched_at: now,
        };
        write_match(&env, &record);
        write_original_of(&env, matched_id, voucher_id);

        account.open_matches.push_back(OpenMatch {
            matched_voucher_id: matched_id,
            amount: match_amount,
        });
        record_movement(&mut account, &mut totals, 0, match_amount, 0, 0)?;
        write_account(&env, &account);
        write_totals(&env, &totals);

        MatchCreated {
            sponsor,
            original_voucher_id: voucher_id,
            matched_voucher_id: matched_id,
            provider,
            service_code,
            amount: match_amount,
            matched_at: now,
        }
        .publish(&env);
        Ok(matched_id)
    }

    /// Gives the sponsor back money it has not matched.
    ///
    /// Only the unused budget can be withdrawn: money that is escrowed in a
    /// matching voucher is not the sponsor's to take back while the care
    /// could still be delivered.
    pub fn withdraw(env: Env, sponsor: Address, amount: i128) -> Result<(), SponsorError> {
        let config = Self::read_config(&env)?;
        sponsor.require_auth();
        if amount <= 0 {
            return Err(SponsorError::InvalidAmount);
        }

        let mut account = read_account(&env, &sponsor);
        let mut totals = read_totals(&env);
        // A refunded match is money that came back: it is unused budget
        // again. The books are brought up to date when the budget looks too
        // small for this withdrawal, which is the only time it can matter.
        let mut available = available_of(&account)?;
        if amount > available {
            Self::reconcile(&env, &config, &mut account, &mut totals)?;
            available = available_of(&account)?;
        }
        if amount > available {
            return Err(SponsorError::WithdrawExceedsBudget);
        }

        let desk = env.current_contract_address();
        token::Client::new(&env, &config.token).transfer(&desk, &sponsor, &amount);

        record_movement(&mut account, &mut totals, 0, 0, 0, amount)?;
        write_account(&env, &account);
        write_totals(&env, &totals);

        BudgetWithdrawn {
            sponsor,
            amount,
            available: available_of(&account)?,
        }
        .publish(&env);
        Ok(())
    }

    /// Brings a sponsor's books up to date with the matching vouchers that
    /// have since settled or been refunded.
    ///
    /// A settled match is money that became care: it stays spent. A refunded
    /// match is money that came back: it is available again. The desk cannot
    /// see a refund happen, so it checks the matching vouchers themselves.
    ///
    /// Permissionless: anyone may call it, and `match_voucher` and
    /// `withdraw` call it for the sponsor they are acting for.
    pub fn reconcile_sponsor(env: Env, sponsor: Address) -> Result<(), SponsorError> {
        let config = Self::read_config(&env)?;
        let mut account = read_account(&env, &sponsor);
        let mut totals = read_totals(&env);
        Self::reconcile(&env, &config, &mut account, &mut totals)?;
        write_account(&env, &account);
        write_totals(&env, &totals);
        Ok(())
    }

    /// Nominates a new admin. The nominee has to accept before the role
    /// moves, so a typo cannot hand it to an address nobody controls.
    pub fn propose_admin(env: Env, admin: Address, new_admin: Address) -> Result<(), SponsorError> {
        Self::require_admin(&env, &admin)?;
        common::propose_admin(&env, &DataKey::PendingAdmin, &new_admin);
        AdminProposed {
            admin,
            pending_admin: new_admin,
        }
        .publish(&env);
        Ok(())
    }

    /// The nominated address accepts and becomes admin.
    pub fn accept_admin(env: Env, new_admin: Address) -> Result<(), SponsorError> {
        let new_admin = common::take_pending_admin(&env, &DataKey::PendingAdmin, &new_admin)?;
        let mut config = Self::read_config(&env)?;
        config.admin = new_admin.clone();
        write_config(&env, &config);
        AdminChanged { new_admin }.publish(&env);
        Ok(())
    }

    /// Pauses or unpauses the desk. While paused, deposits and matches are
    /// rejected for everyone. Withdrawals keep working: pausing stops money
    /// going out, it does not trap money already here.
    pub fn set_paused(env: Env, admin: Address, paused: bool) -> Result<(), SponsorError> {
        let mut config = Self::require_admin(&env, &admin)?;
        config.paused = paused;
        write_config(&env, &config);
        Ok(())
    }

    // ----- views -----

    pub fn get_config(env: Env) -> Result<Config, SponsorError> {
        Self::read_config(&env)
    }

    /// One sponsor's budget and rules.
    ///
    /// A sponsor that has never used the desk reads as an empty account
    /// rather than an error: it simply has no budget and no rules yet.
    pub fn get_sponsor(env: Env, sponsor: Address) -> Result<SponsorAccount, SponsorError> {
        Self::read_config(&env)?;
        Ok(read_account(&env, &sponsor))
    }

    /// The rules a sponsor set, if any.
    pub fn get_rules(env: Env, sponsor: Address) -> Result<Option<Rules>, SponsorError> {
        Self::read_config(&env)?;
        let rules = read_account(&env, &sponsor).rules;
        if rules.match_bps == 0 {
            return Ok(None);
        }
        Ok(Some(rules))
    }

    /// The sponsor's unused budget: what it can still match with, or
    /// withdraw.
    pub fn available_budget(env: Env, sponsor: Address) -> Result<i128, SponsorError> {
        Self::read_config(&env)?;
        available_of(&read_account(&env, &sponsor))
    }

    /// The match a voucher received, if any.
    pub fn get_match(
        env: Env,
        original_voucher_id: u64,
    ) -> Result<Option<MatchRecord>, SponsorError> {
        Ok(read_match(&env, original_voucher_id))
    }

    /// The match a matching voucher belongs to, if any.
    pub fn get_match_of(
        env: Env,
        matched_voucher_id: u64,
    ) -> Result<Option<MatchRecord>, SponsorError> {
        let Some(original_voucher_id) = read_original_of(&env, matched_voucher_id) else {
            return Ok(None);
        };
        Ok(read_match(&env, original_voucher_id))
    }

    /// Read-only solvency check: `(available_budget, token_balance)`.
    ///
    /// `available_budget` is every sponsor's unused budget added together.
    /// `token_balance` is this contract's actual balance of the settlement
    /// token. The two are equal when the accounting is right; a balance that
    /// is larger means a matching voucher was refunded and the books have
    /// not been reconciled yet.
    ///
    /// No authorisation: anyone may run the check.
    pub fn budget_summary(env: Env) -> Result<(i128, i128), SponsorError> {
        let config = Self::read_config(&env)?;
        let totals = read_totals(&env);
        let available = totals
            .deposited
            .checked_sub(totals.committed)
            .and_then(|v| v.checked_add(totals.refunded))
            .and_then(|v| v.checked_sub(totals.withdrawn))
            .ok_or(SponsorError::MathOverflow)?;
        let balance =
            token::Client::new(&env, &config.token).balance(&env.current_contract_address());
        Ok((available, balance))
    }

    pub fn get_pending_admin(env: Env) -> Option<Address> {
        common::read_pending_admin(&env, &DataKey::PendingAdmin)
    }

    /// Crate version this contract was built from, e.g. `"0.2.0"`.
    ///
    /// Lets integrators and the indexer tell deployments apart without
    /// comparing wasm hashes.
    pub fn version(env: Env) -> String {
        String::from_str(&env, env!("CARGO_PKG_VERSION"))
    }

    // ----- internal -----

    fn read_config(env: &Env) -> Result<Config, SponsorError> {
        read_config(env)
    }

    fn require_admin(env: &Env, admin: &Address) -> Result<Config, SponsorError> {
        let config = read_config(env)?;
        if config.admin != *admin {
            return Err(SponsorError::NotAuthorized);
        }
        admin.require_auth();
        Ok(config)
    }

    /// The rules the desk will accept.
    fn check_rules(rules: &Rules) -> Result<(), SponsorError> {
        if rules.match_bps == 0 || rules.match_bps > MAX_MATCH_BPS {
            return Err(SponsorError::InvalidRules);
        }
        if rules.cap_per_voucher <= 0 {
            return Err(SponsorError::InvalidRules);
        }
        if rules.min_voucher_amount < 0 {
            return Err(SponsorError::InvalidRules);
        }
        // ISO 3166-1 alpha-2 is always two characters.
        for country in rules.eligible_countries.iter() {
            if country.len() != 2 {
                return Err(SponsorError::InvalidRules);
            }
        }
        Ok(())
    }

    /// What the sponsor adds for a voucher: their share of the amount, capped.
    fn match_amount(amount: i128, rules: &Rules) -> Result<i128, SponsorError> {
        let share = amount
            .checked_mul(rules.match_bps as i128)
            .ok_or(SponsorError::MathOverflow)?
            .checked_div(BPS_DENOMINATOR)
            .ok_or(SponsorError::MathOverflow)?;
        Ok(core::cmp::min(share, rules.cap_per_voucher))
    }

    /// The sponsor's unused budget, bringing the books up to date first when
    /// that can change the answer.
    ///
    /// A matching voucher that was refunded gives the money back, but the
    /// desk cannot see a refund happen: it has to ask the voucher contract
    /// about every open match, one call each. That is only worth paying for
    /// when the budget looks too small for what is about to happen, or when
    /// the desk needs room for another open match. A sponsor with budget to
    /// spare pays nothing for it.
    fn budget_for(
        env: &Env,
        config: &Config,
        account: &mut SponsorAccount,
        totals: &mut Totals,
        needed: i128,
    ) -> Result<i128, SponsorError> {
        let available = available_of(account)?;
        if available >= needed && account.open_matches.len() < MAX_OPEN_MATCHES {
            return Ok(available);
        }
        Self::reconcile(env, config, account, totals)?;
        available_of(account)
    }

    /// Creates the matching voucher, funded by this contract.
    fn create_matching_voucher(
        env: &Env,
        config: &Config,
        beneficiary_ref: &BytesN<32>,
        provider: &Address,
        service_code: u32,
        amount: i128,
        expires_at: u64,
    ) -> Result<u64, SponsorError> {
        let desk = env.current_contract_address();
        // The voucher contract moves the money out of this contract's
        // balance, so the token call it makes on the way has to be
        // authorised by this contract. The authorisation names that one
        // call and no other: this transfer, of this amount, from this
        // contract to the voucher's escrow.
        Self::authorize_token_transfer(env, config, &desk, amount);

        match VoucherClient::new(env, &config.voucher).try_create_voucher(
            &desk,
            beneficiary_ref,
            provider,
            &service_code,
            &amount,
            &expires_at,
        ) {
            Ok(Ok(id)) => Ok(id),
            Ok(Err(_)) => Err(SponsorError::VoucherCallFailed),
            Err(Ok(VoucherError::AmountBelowPrice)) => Err(SponsorError::MatchBelowPrice),
            Err(Ok(VoucherError::ProviderNotActive)) => Err(SponsorError::ProviderNotEligible),
            Err(Ok(VoucherError::ServiceNotOffered)) => Err(SponsorError::ServiceNotEligible),
            Err(Ok(VoucherError::ServiceUnavailable)) => Err(SponsorError::ServiceUnavailable),
            Err(Err(_)) => Err(SponsorError::VoucherCallFailed),
        }
    }

    /// Authorises the token transfer the voucher contract makes on this
    /// contract's behalf.
    ///
    /// The voucher contract escrows the money by moving it from the funder's
    /// account to its own. When the funder is this contract, that transfer
    /// is made by the voucher contract, not by this one, so this contract
    /// has to authorise it explicitly. The authorisation covers exactly one
    /// call: `transfer` of `amount` from this contract to the voucher
    /// contract, on the configured token.
    fn authorize_token_transfer(env: &Env, config: &Config, from: &Address, amount: i128) {
        // The voucher's escrow is the voucher contract's own address, and
        // the token client takes the destination as a muxed address.
        let escrow = MuxedAddress::from(&config.voucher);
        env.authorize_as_current_contract(vec![
            env,
            InvokerContractAuthEntry::Contract(SubContractInvocation {
                context: ContractContext {
                    contract: config.token.clone(),
                    fn_name: Symbol::new(env, "transfer"),
                    args: vec![
                        env,
                        from.clone().into_val(env),
                        escrow.into_val(env),
                        amount.into_val(env),
                    ],
                },
                sub_invocations: vec![env],
            }),
        ]);
    }

    /// Brings a sponsor's books up to date with the matching vouchers that
    /// have since settled or been refunded.
    fn reconcile(
        env: &Env,
        config: &Config,
        account: &mut SponsorAccount,
        totals: &mut Totals,
    ) -> Result<(), SponsorError> {
        if account.open_matches.is_empty() {
            return Ok(());
        }
        let vouchers = VoucherClient::new(env, &config.voucher);
        let mut still_open = Vec::new(env);
        let mut refunded = 0i128;

        for open in account.open_matches.iter() {
            let Some((_, _, _, _, _, status, _, _)) =
                vouchers.voucher_summary(&open.matched_voucher_id)
            else {
                return Err(SponsorError::VoucherNotFound);
            };
            match status {
                VOUCHER_STATUS_SETTLED => {
                    // The money became care: it stays spent.
                    MatchSettled {
                        sponsor: account.sponsor.clone(),
                        matched_voucher_id: open.matched_voucher_id,
                        amount: open.amount,
                    }
                    .publish(env);
                }
                VOUCHER_STATUS_REFUNDED => {
                    // The money came back: it is available again.
                    refunded = refunded
                        .checked_add(open.amount)
                        .ok_or(SponsorError::MathOverflow)?;
                    MatchRefunded {
                        sponsor: account.sponsor.clone(),
                        matched_voucher_id: open.matched_voucher_id,
                        amount: open.amount,
                    }
                    .publish(env);
                }
                _ => still_open.push_back(open.clone()),
            }
        }

        account.open_matches = still_open;
        if refunded > 0 {
            record_movement(account, totals, 0, 0, refunded, 0)?;
        }
        Ok(())
    }
}
