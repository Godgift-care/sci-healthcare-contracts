//! Measures the ledger size of every stored object and the rent it costs for
//! the TTLs the contracts configure.
//!
//! This is the source of the numbers in `docs/protocol/costs.md`. Run it with
//! `./scripts/storage-costs.sh`; the test prints one line per entry and the
//! doc numbers are asserted against the same values.
//!
//! Sizes are the XDR length of the `LedgerEntry`, which is what Stellar uses
//! for the per-1KB rent calculation (`entry_size_for_rent` in
//! `soroban-env-host`). Rent uses the mainnet rent parameters from
//! `soroban-sdk`'s `CostEstimate` snapshot: 12,000 stroops per KB, divided by
//! 1,215 ledgers for persistent entries and 2,430 for temporary ones. The SDK
//! documents 12,000 as a conservative overestimate, so the numbers here are an
//! upper bound rather than a quote.

use soroban_sdk::{
    xdr::{
        ContractDataDurability, LedgerEntry, LedgerKey, LedgerKeyContractData, Limits, ScAddress,
        ScVal, WriteXdr,
    },
    Address, Env, IntoVal, TryFromVal, Val,
};

use sci_common::{ATTESTATIONS_TTL, INSTANCE_TTL_EXTEND, PERSISTENT_TTL_EXTEND};

use super::*;

/// Rent parameters from `soroban_sdk::testutils::CostEstimate` (mainnet
/// snapshot, 2026-07-10). Kept here as constants so the arithmetic below is
/// plain integers: no floating point in the protocol or in its cost model.
const RENT_STROOPS_PER_KB: i128 = 12_000;
const PERSISTENT_RENT_DENOMINATOR: i128 = 1_215;
const TEMPORARY_RENT_DENOMINATOR: i128 = 2_430;
const DATA_SIZE_1KB_INCREMENT: i128 = 1_024;
const STROOPS_PER_XLM: i128 = 10_000_000;

/// One measured ledger entry.
struct Measurement {
    contract: &'static str,
    entry: &'static str,
    durability: ContractDataDurability,
    ledgers: u32,
    size: u32,
}

impl Measurement {
    fn rent_stroops(&self) -> i128 {
        let denominator = DATA_SIZE_1KB_INCREMENT
            * match self.durability {
                ContractDataDurability::Persistent => PERSISTENT_RENT_DENOMINATOR,
                ContractDataDurability::Temporary => TEMPORARY_RENT_DENOMINATOR,
            };
        // Ceiling division, the same shape as `rent_fee_for_size_and_ledgers`.
        (self.size as i128 * RENT_STROOPS_PER_KB * self.ledgers as i128 + denominator - 1)
            / denominator
    }

    fn rent_xlm(&self) -> std::string::String {
        xlm(self.rent_stroops())
    }
}

fn xlm(stroops: i128) -> std::string::String {
    std::format!(
        "{}.{:07}",
        stroops / STROOPS_PER_XLM,
        stroops % STROOPS_PER_XLM
    )
}

/// Serialises `key` into the `ScVal` a ledger key carries.
fn to_scval<K: IntoVal<Env, Val>>(env: &Env, key: K) -> ScVal {
    let value = key.into_val(env);
    ScVal::try_from_val(env, &value)
        .unwrap_or_else(|_| panic!("could not turn a storage key into ScVal"))
}

fn contract_address(env: &Env, contract: &Address) -> ScAddress {
    match to_scval(env, contract.clone()) {
        ScVal::Address(address) => address,
        other => panic!("contract address is not an address: {other:?}"),
    }
}

/// The XDR length of the ledger entry at `key`, which is the size used for
/// rent. Panics rather than silently reporting zero for a missing entry.
fn entry_size(
    env: &Env,
    contract: &Address,
    key: ScVal,
    durability: ContractDataDurability,
) -> u32 {
    let ledger_key = LedgerKey::ContractData(LedgerKeyContractData {
        contract: contract_address(env, contract),
        key,
        durability,
    });
    let entry: (std::rc::Rc<LedgerEntry>, Option<u32>) = env
        .host()
        .get_ledger_entry(&std::rc::Rc::new(ledger_key.clone()))
        .expect("host could not read the entry")
        .unwrap_or_else(|| panic!("ledger entry is missing: {ledger_key:?}"));
    entry
        .0
        .to_xdr(Limits::none())
        .expect("ledger entry could not be XDR-encoded")
        .len() as u32
}

fn persistent(contract: &'static str, entry: &'static str, size: u32) -> Measurement {
    Measurement {
        contract,
        entry,
        durability: ContractDataDurability::Persistent,
        ledgers: PERSISTENT_TTL_EXTEND,
        size,
    }
}

fn temporary(contract: &'static str, entry: &'static str, size: u32) -> Measurement {
    Measurement {
        contract,
        entry,
        durability: ContractDataDurability::Temporary,
        ledgers: ATTESTATIONS_TTL,
        size,
    }
}

fn instance(contract: &'static str, entry: &'static str, size: u32) -> Measurement {
    Measurement {
        contract,
        entry,
        durability: ContractDataDurability::Persistent,
        ledgers: INSTANCE_TTL_EXTEND,
        size,
    }
}

/// Builds one of every entry the three core contracts store, then measures it.
#[allow(clippy::vec_init_then_push)]
fn measure_all() -> std::vec::Vec<Measurement> {
    let f = setup();

    // Two vouchers: one is settled so the receipt book writes every index it
    // maintains, the other stays open so `Voucher(id)` is a normal Funded
    // voucher at measurement time.
    let settled = fund(&f);
    let open = fund(&f);
    f.voucher.claim(&f.provider, &settled);
    f.voucher.attest(&f.attester, &settled);
    advance(&f.env, DISPUTE_WINDOW + 1);
    f.voucher.settle(&settled);
    f.voucher.set_attestation_cap(&f.admin, &100);

    let patient_ref = patient(&f.env);
    let day = START_TIME / (24 * 60 * 60);

    let mut out = std::vec::Vec::new();

    // Registry: one instance entry plus one entry per provider, service and
    // attester. PendingAdmin lives in instance storage, so it grows the
    // instance entry rather than creating one of its own.
    out.push(instance(
        "registry",
        "contract instance (`Admin`)",
        entry_size(
            &f.env,
            &f.registry.address,
            ScVal::LedgerKeyContractInstance,
            ContractDataDurability::Persistent,
        ),
    ));
    out.push(persistent(
        "registry",
        "`Provider(owner)`",
        entry_size(
            &f.env,
            &f.registry.address,
            to_scval(&f.env, sci_registry::DataKey::Provider(f.provider.clone())),
            ContractDataDurability::Persistent,
        ),
    ));
    out.push(persistent(
        "registry",
        "`Service(provider, code)`",
        entry_size(
            &f.env,
            &f.registry.address,
            to_scval(
                &f.env,
                sci_registry::DataKey::Service(f.provider.clone(), CONSULT),
            ),
            ContractDataDurability::Persistent,
        ),
    ));
    out.push(persistent(
        "registry",
        "`Attester(address)`",
        entry_size(
            &f.env,
            &f.registry.address,
            to_scval(&f.env, sci_registry::DataKey::Attester(f.attester.clone())),
            ContractDataDurability::Persistent,
        ),
    ));

    // Voucher: the instance holds Config, NextId, OpenEscrow and the
    // attestation cap; each voucher is its own persistent entry and each
    // attester/day counter is temporary.
    out.push(instance(
        "voucher",
        "contract instance (`Config`, `NextId`, `OpenEscrow`, `AttestationCap`)",
        entry_size(
            &f.env,
            &f.voucher.address,
            ScVal::LedgerKeyContractInstance,
            ContractDataDurability::Persistent,
        ),
    ));
    out.push(persistent(
        "voucher",
        "`Voucher(id)`",
        entry_size(
            &f.env,
            &f.voucher.address,
            to_scval(&f.env, crate::DataKey::Voucher(open)),
            ContractDataDurability::Persistent,
        ),
    ));
    out.push(temporary(
        "voucher",
        "`Attestations(attester, day)`",
        entry_size(
            &f.env,
            &f.voucher.address,
            to_scval(
                &f.env,
                crate::DataKey::Attestations(f.attester.clone(), day),
            ),
            ContractDataDurability::Temporary,
        ),
    ));

    // Receipt: the instance holds Admin and Minter; each receipt, beneficiary
    // count, provider count and provider index entry is persistent.
    out.push(instance(
        "receipt",
        "contract instance (`Admin`, `Minter`)",
        entry_size(
            &f.env,
            &f.receipts.address,
            ScVal::LedgerKeyContractInstance,
            ContractDataDurability::Persistent,
        ),
    ));
    out.push(persistent(
        "receipt",
        "`Receipt(voucher_id)`",
        entry_size(
            &f.env,
            &f.receipts.address,
            to_scval(&f.env, sci_receipt::DataKey::Receipt(settled)),
            ContractDataDurability::Persistent,
        ),
    ));
    out.push(persistent(
        "receipt",
        "`Count(beneficiary_ref)`",
        entry_size(
            &f.env,
            &f.receipts.address,
            to_scval(&f.env, sci_receipt::DataKey::Count(patient_ref.clone())),
            ContractDataDurability::Persistent,
        ),
    ));
    out.push(persistent(
        "receipt",
        "`ProviderCount(provider)`",
        entry_size(
            &f.env,
            &f.receipts.address,
            to_scval(
                &f.env,
                sci_receipt::DataKey::ProviderCount(f.provider.clone()),
            ),
            ContractDataDurability::Persistent,
        ),
    ));
    out.push(persistent(
        "receipt",
        "`ProviderReceipt(provider, index)`",
        entry_size(
            &f.env,
            &f.receipts.address,
            to_scval(
                &f.env,
                sci_receipt::DataKey::ProviderReceipt(f.provider.clone(), 0),
            ),
            ContractDataDurability::Persistent,
        ),
    ));

    out
}

#[test]
fn measured_storage_costs_match_the_docs() {
    let measurements = measure_all();

    // Printed for `scripts/storage-costs.sh`; the docs test below reads the
    // same values, so a table that drifts from the code fails CI.
    for m in &measurements {
        std::println!(
            "cost\t{}\t{}\t{}\t{}\t{}\t{}",
            m.contract,
            m.entry,
            match m.durability {
                ContractDataDurability::Persistent => "persistent",
                ContractDataDurability::Temporary => "temporary",
            },
            m.ledgers,
            m.size,
            m.rent_stroops(),
        );
    }

    let docs = include_str!("../../../../docs/protocol/costs.md");
    for m in &measurements {
        let row = std::format!(
            "| `{}` | {} | {} | {} | {} | {} |",
            m.contract,
            m.entry,
            match m.durability {
                ContractDataDurability::Persistent => "persistent",
                ContractDataDurability::Temporary => "temporary",
            },
            m.ledgers,
            m.size,
            m.rent_xlm(),
        );
        assert!(
            docs.contains(&row),
            "docs/protocol/costs.md is missing or has drifted from this row:\n{row}\n\
             Regenerate the table with ./scripts/storage-costs.sh rather than \
             editing the number by hand",
        );
    }
}
