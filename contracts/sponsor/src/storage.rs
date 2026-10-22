use sci_common as common;
use soroban_sdk::{Address, Env};

use crate::types::{Config, DataKey, MatchRecord, Rules, SponsorAccount, SponsorError, Totals};

pub(crate) fn extend_instance(env: &Env) {
    common::extend_instance(env);
}

pub(crate) fn has_config(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Config)
}

pub(crate) fn read_config(env: &Env) -> Result<Config, SponsorError> {
    env.storage()
        .instance()
        .get(&DataKey::Config)
        .ok_or(SponsorError::NotInitialized)
}

pub(crate) fn write_config(env: &Env, config: &Config) {
    env.storage().instance().set(&DataKey::Config, config);
}

/// One sponsor's account, or a fresh empty one for a sponsor that has not
/// used the desk yet.
pub(crate) fn read_account(env: &Env, sponsor: &Address) -> SponsorAccount {
    let key = DataKey::Sponsor(sponsor.clone());
    let account: Option<SponsorAccount> = env.storage().persistent().get(&key);
    match account {
        Some(account) => {
            common::extend_persistent(env, &key);
            account
        }
        None => SponsorAccount {
            sponsor: sponsor.clone(),
            deposited: 0,
            committed: 0,
            refunded: 0,
            withdrawn: 0,
            rules: empty_rules(env),
            open_matches: soroban_sdk::Vec::new(env),
        },
    }
}

/// The rules of a sponsor that has not set any.
pub(crate) fn empty_rules(env: &Env) -> Rules {
    Rules {
        match_bps: 0,
        cap_per_voucher: 0,
        min_voucher_amount: 0,
        min_voucher_age_secs: 0,
        eligible_countries: soroban_sdk::Vec::new(env),
        eligible_service_codes: soroban_sdk::Vec::new(env),
    }
}

pub(crate) fn write_account(env: &Env, account: &SponsorAccount) {
    let key = DataKey::Sponsor(account.sponsor.clone());
    env.storage().persistent().set(&key, account);
    common::extend_persistent(env, &key);
    extend_instance(env);
}

/// The match a voucher received, if any.
pub(crate) fn read_match(env: &Env, original_voucher_id: u64) -> Option<MatchRecord> {
    let key = DataKey::Match(original_voucher_id);
    let record: Option<MatchRecord> = env.storage().persistent().get(&key);
    if record.is_some() {
        common::extend_persistent(env, &key);
    }
    record
}

pub(crate) fn write_match(env: &Env, record: &MatchRecord) {
    let key = DataKey::Match(record.original_voucher_id);
    env.storage().persistent().set(&key, record);
    common::extend_persistent(env, &key);
    extend_instance(env);
}

/// The original voucher a matching voucher belongs to, if any.
pub(crate) fn read_original_of(env: &Env, matched_voucher_id: u64) -> Option<u64> {
    let key = DataKey::OriginalOf(matched_voucher_id);
    let original: Option<u64> = env.storage().persistent().get(&key);
    if original.is_some() {
        common::extend_persistent(env, &key);
    }
    original
}

pub(crate) fn write_original_of(env: &Env, matched_voucher_id: u64, original_voucher_id: u64) {
    let key = DataKey::OriginalOf(matched_voucher_id);
    env.storage().persistent().set(&key, &original_voucher_id);
    common::extend_persistent(env, &key);
}

/// The desk's totals across every sponsor.
pub(crate) fn read_totals(env: &Env) -> Totals {
    env.storage()
        .instance()
        .get(&DataKey::Totals)
        .unwrap_or(Totals {
            deposited: 0,
            committed: 0,
            refunded: 0,
            withdrawn: 0,
        })
}

pub(crate) fn write_totals(env: &Env, totals: &Totals) {
    env.storage().instance().set(&DataKey::Totals, totals);
    extend_instance(env);
}

/// A sponsor's unused budget: what is still theirs to match with or to
/// withdraw.
///
/// `deposited - committed + refunded - withdrawn`. A refunded match is money
/// that came back, so it is available again; a settled one is money that
/// became care, so it is not.
pub(crate) fn available_of(account: &SponsorAccount) -> Result<i128, SponsorError> {
    let committed = account
        .deposited
        .checked_sub(account.committed)
        .ok_or(SponsorError::MathOverflow)?;
    let refunded = committed
        .checked_add(account.refunded)
        .ok_or(SponsorError::MathOverflow)?;
    refunded
        .checked_sub(account.withdrawn)
        .ok_or(SponsorError::MathOverflow)
}

/// A sponsor's budget, and the desk's totals, moved together.
///
/// Every path that moves money updates both, so the solvency view and the
/// per-sponsor view always agree.
pub(crate) fn record_movement(
    account: &mut SponsorAccount,
    totals: &mut Totals,
    deposited: i128,
    committed: i128,
    refunded: i128,
    withdrawn: i128,
) -> Result<(), SponsorError> {
    account.deposited = account
        .deposited
        .checked_add(deposited)
        .ok_or(SponsorError::MathOverflow)?;
    account.committed = account
        .committed
        .checked_add(committed)
        .ok_or(SponsorError::MathOverflow)?;
    account.refunded = account
        .refunded
        .checked_add(refunded)
        .ok_or(SponsorError::MathOverflow)?;
    account.withdrawn = account
        .withdrawn
        .checked_add(withdrawn)
        .ok_or(SponsorError::MathOverflow)?;

    totals.deposited = totals
        .deposited
        .checked_add(deposited)
        .ok_or(SponsorError::MathOverflow)?;
    totals.committed = totals
        .committed
        .checked_add(committed)
        .ok_or(SponsorError::MathOverflow)?;
    totals.refunded = totals
        .refunded
        .checked_add(refunded)
        .ok_or(SponsorError::MathOverflow)?;
    totals.withdrawn = totals
        .withdrawn
        .checked_add(withdrawn)
        .ok_or(SponsorError::MathOverflow)?;
    Ok(())
}
