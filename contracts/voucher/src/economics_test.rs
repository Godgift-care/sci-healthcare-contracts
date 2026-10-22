#![cfg(test)]

//! Keeps the fee tables in `docs/protocol/economics.md` honest: every row is
//! recomputed with `quote` at the production fee (100 bps), so a fee change
//! or a typo in the docs fails here instead of misleading a funder or clinic.

extern crate std;

use soroban_sdk::{testutils::Address as _, Address, Env};

use crate::{VoucherEscrow, VoucherEscrowClient};

const DOCS: &str = include_str!("../../../docs/protocol/economics.md");
/// The production fee the tables are written for; `scripts/deploy.sh`
/// deploys with it.
const FEE_BPS: u32 = 100;
/// The attester's slice of the voucher amount in the production deployment:
/// half of the 100 bps fee.
const ATTESTER_SHARE_BPS: u32 = 50;
const STROOPS_PER_USDC: i128 = 10_000_000;

fn quoting_contract(env: &Env) -> VoucherEscrowClient<'_> {
    env.mock_all_auths();
    let a = Address::generate(env);
    let address = env.register(VoucherEscrow, ());
    let client = VoucherEscrowClient::new(env, &address);
    client.initialize(
        &a,
        &a,
        &a,
        &a,
        &(72 * 60 * 60),
        &FEE_BPS,
        &ATTESTER_SHARE_BPS,
        &a,
    );
    client
}

/// The data rows of the markdown table that follows `heading`.
fn table_rows<'a>(heading: &str) -> std::vec::Vec<std::vec::Vec<&'a str>> {
    let start = DOCS
        .find(heading)
        .unwrap_or_else(|| panic!("economics.md has no `{heading}` section"));
    DOCS[start..]
        .lines()
        .skip_while(|line| !line.starts_with('|'))
        .take_while(|line| line.starts_with('|'))
        .skip(2) // header and separator
        .map(|line| {
            line.trim_matches('|')
                .split('|')
                .map(str::trim)
                .collect::<std::vec::Vec<_>>()
        })
        .collect()
}

/// `12_345` or `12,345` as an integer.
fn stroops(cell: &str) -> i128 {
    let digits: std::string::String = cell.chars().filter(char::is_ascii_digit).collect();
    digits
        .parse()
        .unwrap_or_else(|_| panic!("`{cell}` is not a stroop amount"))
}

/// `$3.00` (cents only) as stroops: one dollar is one USDC.
fn dollars(cell: &str) -> i128 {
    let text = cell
        .split_whitespace()
        .next()
        .unwrap_or_else(|| panic!("empty dollar cell"))
        .trim_start_matches('$');
    let (whole, cents) = text
        .split_once('.')
        .unwrap_or_else(|| panic!("`{cell}` is not a $x.yy amount"));
    let whole: i128 = whole.parse().unwrap();
    let cents: i128 = cents.parse().unwrap();
    whole * STROOPS_PER_USDC + cents * STROOPS_PER_USDC / 100
}

#[test]
fn settlement_examples_table_matches_quote() {
    let env = Env::default();
    let voucher = quoting_contract(&env);

    let rows = table_rows("### Settlement examples in stroops");
    assert!(rows.len() >= 4, "the settlement table lost rows");
    for row in rows {
        let amount = stroops(row[1]);
        let (fee, net) = voucher.quote(&amount);
        assert_eq!(
            (stroops(row[2]), stroops(row[3])),
            (fee, net),
            "economics.md says {} pays a fee of {} and nets {}, but quote({amount}) is ({fee}, {net})",
            row[0],
            row[2],
            row[3],
        );
        assert_eq!(fee + net, amount, "fee and net must add up to the voucher");
    }
}

#[test]
fn attester_share_table_matches_the_settlement_split() {
    let env = Env::default();
    let voucher = quoting_contract(&env);

    let rows = table_rows("### Attester share at 50 bps");
    assert!(!rows.is_empty(), "the attester share table lost its rows");
    for row in rows {
        let amount = stroops(row[1]);
        let (fee, _) = voucher.quote(&amount);
        let attester_share = amount * ATTESTER_SHARE_BPS as i128 / 10_000;
        let fee_account_share = fee - attester_share;
        assert_eq!(
            (stroops(row[2]), stroops(row[3])),
            (attester_share, fee_account_share),
            "economics.md attester share row is wrong",
        );
        assert_eq!(
            attester_share + fee_account_share,
            fee,
            "attester share and fee account share must add up to the fee",
        );
        assert!(
            attester_share <= fee,
            "attester share cannot exceed the fee",
        );
    }
}

#[test]
fn worked_numbers_table_matches_quote() {
    let env = Env::default();
    let voucher = quoting_contract(&env);

    let rows = table_rows("### Worked numbers at 1%");
    assert!(!rows.is_empty(), "the worked numbers table lost its rows");
    for row in rows {
        let amount = dollars(row[0]);
        let (fee, net) = voucher.quote(&amount);
        assert_eq!(
            (dollars(row[1]), dollars(row[2])),
            (fee, net),
            "economics.md row `{}` disagrees with quote({amount}) = ({fee}, {net})",
            row[0],
        );
    }
}
