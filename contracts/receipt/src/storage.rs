use soroban_sdk::{contracttype, Address, BytesN, Env, TryFromVal, Val, Vec};

use crate::{DataKey, Receipt, ReceiptError};

/// How a receipt is laid out in storage.
///
/// `Receipt` is a named struct, which the host encodes as an XDR map: every
/// field is stored with its name as a symbol key, so eight field names ride
/// along in every receipt entry and are paid for in rent for its whole life.
/// This tuple struct is encoded as an XDR vec — values only, in order.
///
/// Field order is part of the storage format: never reorder, only append.
///
/// Not in the contract spec: it is `pub(crate)` and never crosses the
/// contract boundary, which is what soroban-sdk 29 keys the spec off.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StoredReceipt(
    u64,        // voucher_id
    BytesN<32>, // beneficiary_ref
    Address,    // provider
    u32,        // service_code
    i128,       // amount
    i128,       // fee
    i128,       // net
    u64,        // settled_at
);

/// The named layout every receipt minted before `fee` and `net` were recorded
/// was written with.
///
/// Kept only so a receipt written by an older deployment still decodes. New
/// writes always use `StoredReceipt`; a read falls back to this and reports
/// `fee = 0` and `net = amount`, meaning the split was never recorded on that
/// receipt. The invariant `net + fee == amount` still holds for it.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StoredReceiptV1 {
    pub(crate) voucher_id: u64,
    pub(crate) beneficiary_ref: BytesN<32>,
    pub(crate) provider: Address,
    pub(crate) service_code: u32,
    pub(crate) amount: i128,
    pub(crate) settled_at: u64,
}

impl StoredReceipt {
    pub(crate) fn from_receipt(r: &Receipt) -> Self {
        StoredReceipt(
            r.voucher_id,
            r.beneficiary_ref.clone(),
            r.provider.clone(),
            r.service_code,
            r.amount,
            r.fee,
            r.net,
            r.settled_at,
        )
    }

    pub(crate) fn into_receipt(self) -> Receipt {
        Receipt {
            voucher_id: self.0,
            beneficiary_ref: self.1,
            provider: self.2,
            service_code: self.3,
            amount: self.4,
            fee: self.5,
            net: self.6,
            settled_at: self.7,
        }
    }
}

impl StoredReceiptV1 {
    pub(crate) fn into_receipt(self) -> Receipt {
        Receipt {
            voucher_id: self.voucher_id,
            beneficiary_ref: self.beneficiary_ref,
            provider: self.provider,
            service_code: self.service_code,
            amount: self.amount,
            // The fee was not on the receipt: the gross amount was all it
            // recorded, so the clinic's known net is the whole of it.
            fee: 0,
            net: self.amount,
            settled_at: self.settled_at,
        }
    }
}

/// Reads a receipt written by this or an earlier release.
///
/// The stored value is inspected before it is decoded: an old receipt is a
/// named map and a current one is a positional vec, so a conversion into the
/// wrong layout would fail at best and trap inside the host at worst.
pub(crate) fn read_receipt(env: &Env, voucher_id: u64) -> Result<Receipt, ReceiptError> {
    let key = DataKey::Receipt(voucher_id);
    let raw: Val = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(ReceiptError::ReceiptNotFound)?;

    if let Ok(fields) = Vec::<Val>::try_from_val(env, &raw) {
        if fields.len() as usize != 8 {
            return Err(ReceiptError::ReceiptNotFound);
        }
        return StoredReceipt::try_from_val(env, &raw)
            .map(|stored| stored.into_receipt())
            .map_err(|_| ReceiptError::ReceiptNotFound);
    }

    StoredReceiptV1::try_from_val(env, &raw)
        .map(|old| old.into_receipt())
        .map_err(|_| ReceiptError::ReceiptNotFound)
}

pub(crate) fn write_receipt(env: &Env, receipt: &Receipt) {
    let key = DataKey::Receipt(receipt.voucher_id);
    env.storage()
        .persistent()
        .set(&key, &StoredReceipt::from_receipt(receipt));
    sci_common::extend_persistent(env, &key);
}
