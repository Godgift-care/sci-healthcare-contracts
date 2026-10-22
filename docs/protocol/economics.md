---
title: Economics
nav_order: 3
---

# Economics

## The fee

The protocol takes a fee in basis points on settlement only. The production configuration is **100 basis points — 1%**. The contract caps it at 1000 bps (10%) and this ceiling is enforced at initialisation, so a misconfigured deployment cannot charge more.

```
fee = amount * fee_bps / 10_000
net = amount - fee
```

All integer arithmetic. There are no floating point numbers anywhere in this protocol. Division truncates, and truncation always favours the clinic rather than the protocol.

### The attester share

Attesters do the work that makes the protocol trustworthy, so settlement pays them a configurable part of the fee. `attester_share_bps` is set once at initialisation alongside `fee_bps`, and the contract rejects any value above `fee_bps`: the share is carved out of the fee, never charged on top of it.

```
attester_share = amount * attester_share_bps / 10_000
fee_account    = fee - attester_share
```

The clinic's `net` does not change. The three payouts always sum to the voucher amount exactly:

```
net + attester_share + fee_account = amount
```

The production deployment sets `fee_bps = 100` and `attester_share_bps = 50`, so half the fee goes to the attester who confirmed delivery and half to the fee account. `attester_share_bps = 0` pays the fee account in full, exactly as the contract behaved before the attester share existed. A disputed claim that the admin resolves for the clinic without an attestation has no attester of record, so that settlement also pays the whole fee to the fee account.

### Worked numbers at 1%

| Voucher | Fee | Clinic receives |
| --- | --- | --- |
| $1.00 (malaria rapid test) | $0.01 | $0.99 |
| $3.00 (outpatient consult) | $0.03 | $2.97 |
| $5.00 (antenatal visit) | $0.05 | $4.95 |
| $25.00 (minor procedure) | $0.25 | $24.75 |

Before the attester share was introduced, a $3.00 voucher settled to exactly `29700000` stroops for the clinic and `300000` for the fee account. With the production split above, that `300000` fee becomes `150000` for the attester and `150000` for the fee account; the clinic still receives `29700000`.

### Settlement examples in stroops

What `quote` returns, and what a settlement pays, at the production fee of 100 bps. USDC on Stellar has 7 decimals, so 1 USDC is `10,000,000` stroops; the contract works in stroops throughout.

| Voucher | Amount (stroops) | Fee (stroops) | Clinic receives (stroops) |
| --- | --- | --- | --- |
| 1 USDC | 10,000,000 | 100,000 | 9,900,000 |
| 3 USDC | 30,000,000 | 300,000 | 29,700,000 |
| 10 USDC | 100,000,000 | 1,000,000 | 99,000,000 |
| 100 USDC | 1,000,000,000 | 10,000,000 | 990,000,000 |
| 1.2345678 USDC | 12,345,678 | 123,456 | 12,222,222 |
| 0.0000099 USDC | 99 | 0 | 99 |

The last two rows show the truncation: `12,345,678 × 100 / 10,000` is `123,456.78`, and the `.78` stays with the clinic; on a 99-stroop voucher the fee rounds down to nothing. A funder who pays more than the listed price is quoted on what they actually pay.

### Attester share at 50 bps

How the production fee splits at settlement: the clinic still receives `amount - fee`, the attester receives its share of the amount, and the fee account receives the rest. The attester share is part of the fee, not an extra deduction.

| Voucher | Amount (stroops) | Attester receives (stroops) | Fee account receives (stroops) |
| --- | --- | --- | --- |
| 1 USDC | 10,000,000 | 50,000 | 50,000 |
| 3 USDC | 30,000,000 | 150,000 | 150,000 |
| 10 USDC | 100,000,000 | 500,000 | 500,000 |
| 100 USDC | 1,000,000,000 | 5,000,000 | 5,000,000 |
| 1.2345678 USDC | 12,345,678 | 61,728 | 61,728 |
| 0.0000099 USDC | 99 | 0 | 0 |

The last row is the same truncation in miniature: a 99-stroop voucher has no fee to split, so neither the attester nor the fee account receives anything.

All three tables on this page are checked against the contract: `settlement_examples_table_matches_quote`, `worked_numbers_table_matches_quote` and `attester_share_table_matches_the_settlement_split` in `contracts/voucher/src/economics_test.rs` recompute every row and fail if a number here is wrong. To check one by hand on a deployment:

```bash
stellar contract invoke --id "$VOUCHER_CONTRACT_ID" --source-account sci-admin \
  --network testnet --send=no -- quote --amount 30000000
# ["300000","29700000"]
```

### What is not charged

- **Refunds are free.** An expired or successfully disputed voucher returns the full amount. No fee is taken on care that did not happen.
- **Reducing an overpayment is free.** `reduce_amount` returns the difference between what was escrowed and the clinic's listed price to the funder. No fee is taken on money that never became care.
- **Funding is free.** The fee is taken once, at settlement.
- **The attester share is part of the fee.** It comes out of the protocol fee, not out of the clinic's net, so configuring it never reduces what the clinic receives.
- **No spread.** The protocol does not convert currency and takes no exchange margin.

## Network costs

Stellar transaction fees are paid in XLM and are a fraction of a cent. At the time of writing the base fee is 100 stroops — 0.00001 XLM — per operation.

This matters more than it sounds. A voucher's whole lifecycle is four transactions: create, claim, attest, settle. On a chain where a transaction costs $0.50, that is $2.00 of overhead on a $3.00 consultation, and the product does not exist. Here it is a rounding error.

## Who pays what

| Action | Who submits | Who pays the network fee |
| --- | --- | --- |
| `create_voucher` | Funder | Funder |
| `reduce_amount` | Funder | Funder |
| `claim` | Clinic | Clinic |
| `attest` | Attester | Attester |
| `settle` | Anyone | Whoever submits |
| `refund` | Anyone | Whoever submits |

Because `settle` and `refund` are permissionless, an operator can run a keeper that settles eligible vouchers on everyone's behalf and absorbs those fees. Nothing in the protocol depends on that keeper existing — it is a convenience, not a dependency.

## Why there is no float

The contract holds no balances and has no deposit function. Money enters only as a voucher already bound to a clinic, a service and an expiry, and it can only leave to that clinic or back to the funder.

This is a deliberate design constraint rather than an oversight, and it is the difference between a prepaid voucher and stored value. A general reloadable balance is e-money in most jurisdictions and brings licensing with it. A prepaid voucher for an identified provider and an identified service generally falls under limited-network carve-outs.

The protocol also **bears no risk**. It does not pool premiums, does not underwrite, and does not pay out on a health event. The moment a system takes money in and promises to pay for a future uncertain medical need, it is insurance and needs a licence. This protocol only ever moves money a funder has already committed to a specific purchase.
