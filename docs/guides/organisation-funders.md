---
title: Funding from an organisation wallet
nav_order: 7
---

# Funding from an organisation wallet

An organisation — an NGO, an employer, a clinic network or a remittance
partner — usually pays for many vouchers from one Stellar account. That account
holds the organisation's USDC, so it should not be one person's key. This guide
sets up a shared **2-of-3 multisig** funder account and covers every action the
organisation can take on a voucher: who signs, what the signature authorises,
where refunds go, and what to record for each voucher.

If you are funding one voucher from your own wallet, read
[Funding care for someone](funders.md) instead. If you are setting up the
protocol's **admin** account, read
[Using a multisig account as admin](../developers/multisig-admin.md). The two
accounts are deliberately different: the admin can suspend clinics and resolve
disputes, while a funder can only spend its own money.

## What changes when an organisation funds

The contract records one `funder: Address` on each voucher. That address is the
organisation's shared account, not the staff member who prepared the payment.
Three things follow:

- **Approval is on chain.** A `create_voucher` call needs the funder's
  signature. With a 2-of-3 account, two people approve every payment, and the
  Stellar network enforces that before the contract runs.
- **Refunds return to the shared account.** `refund` pays the address stored in
  the voucher's `funder` field. Nobody can redirect a refund to a different
  account — not the keeper, not the clinic, not the admin.
- **The organisation keeps the key to its own money.** There is no custodial
  account to recover. If two of the three signers are lost, the remaining funds
  are unrecoverable, so back up the keys and rehearse a restore.

The protocol never holds a balance for the organisation. USDC sits either in the
organisation's wallet or in the voucher contract's escrow for a named voucher.
There is no pooled organisation balance inside the protocol.

## 1. Set up the funder multisig

Use three signers, each held by a different person on a different device, in a
different place. Never put all three keys on one machine, and never reuse a
protocol admin signer as a funder signer.

The commands below are the testnet walkthrough. They assume the Stellar CLI is
installed; see [Local setup](../developers/setup.md) if it is not. Run them
from the repository root, so `deployments/testnet.env` resolves.

```bash
NETWORK=testnet
ORG=sci-funder-ms

# 1. Create the account and three signer identities.
stellar keys generate --network "$NETWORK" --fund "$ORG"
for n in 1 2 3; do
  stellar keys generate --network "$NETWORK" --fund "sci-funder-signer-$n"
done

ORG_ADDR=$(stellar keys address "$ORG")
```

Add each signer with weight `1`. `set-options` accepts one signer per
transaction, so this is three transactions:

```bash
for n in 1 2 3; do
  address=$(stellar keys address "sci-funder-signer-$n")
  stellar tx new set-options \
    --source-account "$ORG" \
    --network "$NETWORK" \
    --signer "$address" \
    --signer-weight 1
done
```

Set every threshold to `2`, leaving the master key at weight `1` for now:

```bash
stellar tx new set-options \
  --source-account "$ORG" \
  --network "$NETWORK" \
  --low-threshold 2 \
  --med-threshold 2 \
  --high-threshold 2
```

Before disabling the master key, prove that two signers can act and one cannot.
Build a no-op transaction, sign it with signers 1 and 2, and send it:

```bash
stellar tx new set-options \
  --source-account "$ORG_ADDR" \
  --network "$NETWORK" \
  --master-weight 1 \
  --build-only > /tmp/org-test.build.xdr

stellar tx sign /tmp/org-test.build.xdr \
  --sign-with-key sci-funder-signer-1 --network "$NETWORK" > /tmp/org-test.1.xdr

stellar tx sign /tmp/org-test.1.xdr \
  --sign-with-key sci-funder-signer-2 --network "$NETWORK" > /tmp/org-test.2.xdr

stellar tx send /tmp/org-test.2.xdr --network "$NETWORK"
```

Then repeat with only signer 1. It must fail with a signature/threshold error.
Only after both outcomes are confirmed, disable the master key with two
signatures:

```bash
stellar tx new set-options \
  --source-account "$ORG_ADDR" \
  --network "$NETWORK" \
  --master-weight 0 \
  --build-only > /tmp/org-disable-master.build.xdr

stellar tx sign /tmp/org-disable-master.build.xdr \
  --sign-with-key sci-funder-signer-1 --network "$NETWORK" > /tmp/org-disable-master.1.xdr

stellar tx sign /tmp/org-disable-master.1.xdr \
  --sign-with-key sci-funder-signer-2 --network "$NETWORK" > /tmp/org-disable-master.2.xdr

stellar tx send /tmp/org-disable-master.2.xdr --network "$NETWORK"
```

Check the result before moving any money:

```bash
curl -s "https://horizon-testnet.stellar.org/accounts/$ORG_ADDR" \
  | jq '{thresholds, signers: [.signers[] | {key, weight}]}'
```

The three signers should have weight `1`, every threshold should be `2`, and the
master key should have weight `0` (or be absent). From this point every
transaction from the account needs two of the three signers — including the
trustline and fee top-ups below.

> **Do not skip the two-signature test.** If the master key is disabled before
> two signers are proven to work, one misconfigured signer can lock the account
> permanently.

### Add the USDC trustline

A Stellar account must opt in to holding USDC. The testnet issuer is published
in [`deployments/testnet.env`](../../deployments/testnet.env); the mainnet issuer
will be announced before launch.

```bash
set -a
source deployments/testnet.env
set +a

stellar tx new change-trust \
  --source-account "$ORG_ADDR" \
  --network "$NETWORK" \
  --line "USDC:$USDC_ISSUER" \
  --build-only > /tmp/org-trustline.build.xdr

stellar tx sign /tmp/org-trustline.build.xdr \
  --sign-with-key sci-funder-signer-1 --network "$NETWORK" > /tmp/org-trustline.1.xdr

stellar tx sign /tmp/org-trustline.1.xdr \
  --sign-with-key sci-funder-signer-2 --network "$NETWORK" > /tmp/org-trustline.2.xdr

stellar tx send /tmp/org-trustline.2.xdr --network "$NETWORK"
```

`change-trust` is a classic Stellar operation, so it needs signatures but no
Soroban simulation. Contract calls below do need simulation, because it attaches
the ledger footprint and the authorisation entry.

### Get test USDC and keep an XLM float

On testnet, call the faucet with the organisation account as both source and
recipient. `drip` requires the recipient's authorisation, which the two envelope
signatures provide:

```bash
stellar contract invoke \
  --id "$FAUCET_CONTRACT_ID" \
  --source-account "$ORG_ADDR" \
  --network "$NETWORK" \
  --build-only \
  -- drip --to "$ORG_ADDR" > /tmp/org-drip.build.xdr

stellar tx simulate /tmp/org-drip.build.xdr \
  --source-account "$ORG_ADDR" \
  --network "$NETWORK" > /tmp/org-drip.xdr

stellar tx sign /tmp/org-drip.xdr \
  --sign-with-key sci-funder-signer-1 --network "$NETWORK" > /tmp/org-drip.1.xdr

stellar tx sign /tmp/org-drip.1.xdr \
  --sign-with-key sci-funder-signer-2 --network "$NETWORK" > /tmp/org-drip.2.xdr

stellar tx send /tmp/org-drip.2.xdr --network "$NETWORK"
```

On mainnet there is no faucet. Fund the account from the organisation's treasury
and keep enough XLM for fees and the account's reserves. USDC pays for care;
XLM pays for transactions. Running out of XLM does not lose escrow, but it stops
the organisation from funding, disputing or reducing vouchers until it is topped
up. Monitor the XLM balance like any other treasury balance.

## 2. Who signs what

Each funder contract call has one required signer, documented in the
[authorisation matrix](../developers/contracts.md#authorisation-matrix). For a
2-of-3 funder account, the transaction source is the organisation account, so
the two envelope signatures satisfy that one required signer.

### Voucher actions

| Action | Contract call | Required signer | What the organisation needs | Notes |
| --- | --- | --- | --- | --- |
| Fund a voucher | `create_voucher(funder, beneficiary_ref, provider, service_code, amount, expires_at)` | `funder` | **2 of 3 signers** | The voucher's funder is the organisation account |
| Fund with options | `create_voucher_with_options(...)` | `funder` | **2 of 3 signers** | Same, plus a presence commitment and/or a per-voucher dispute window |
| Recover an overpayment | `reduce_amount(funder, voucher_id, new_amount)` | `funder` | **2 of 3 signers** | Only while `Funded`; difference returns to the organisation |
| Move a voucher to another clinic | `redirect(funder, voucher_id, new_provider)` | `funder` | **2 of 3 signers** | Only while `Funded`; amount, expiry and escrow do not change |
| Dispute a claim | `dispute(funder, voucher_id, reason_code)` | `funder` | **2 of 3 signers** | Only while `Claimed`, or `Attested` before the dispute deadline |
| Receive a refund | `refund(voucher_id)` | **none** | **No signature** | Permissionless; full amount goes to the stored funder address |
| Receive a reduction | part of `reduce_amount` | — | **No separate call** | Difference is transferred during the reduction |
| Receive a dispute refund | part of `resolve_dispute` | — | **No separate call** | Admin decides; refund goes to the stored funder address |
| Watch status | `get_voucher`, `refundable_at`, API reads | none | No signature | Read-only |

`settle` is also permissionless, but it is not an organisation action: it pays
the clinic and the attester after the dispute window closes. A keeper or the
clinic can call it.

Two rules keep the signatures meaningful:

1. **The transaction source must be the organisation account, and the `funder`
   argument must be that same account.** If the `funder` argument is a different
   address, the two envelope signatures authorise the fee payer, not that
   funder, and the call fails with `NotAuthorized`.
2. **Never sign an XDR you have not decoded.** Before adding a signature, run
   `stellar tx decode` on the file and check the contract id, function name,
   `funder`, provider, amount, service code and expiry. A URL or a message from a
   colleague is not a payment instruction; the decoded transaction is.

### Account actions

Because all three thresholds are `2`, these also need two signers:

| Action | Command | Signers |
| --- | --- | --- |
| Add or remove a signer | `stellar tx new set-options --signer ... --signer-weight ...` | 2 existing signers |
| Change thresholds | `stellar tx new set-options --*-threshold ...` | 2 existing signers |
| Add or remove the USDC trustline | `stellar tx new change-trust ...` | 2 signers |
| Pay a fee, top up XLM, or move USDC outside a voucher | `stellar tx new payment ...` | 2 signers |
| Rotate the master key | `stellar tx new set-options --master-weight ...` | 2 existing signers |

Keep an emergency procedure: with 2-of-3, one lost signer can be removed and
replaced by the other two. Two lost signers means the account is gone unless
there is an off-chain backup. Store the signer seeds separately, in sealed
backups, and test recovery before the account holds real money.

## 3. Fund a voucher

This is the complete flow. In production, signer 1 and signer 2 are on
different machines and pass the partly signed XDR between them; the commands
below show the sequence on one testnet machine.

Set the values. `beneficiary_ref` is a 32-byte hex value produced by the app from
the patient's reference and secret; do not invent it from a name or a phone
number. Amounts are in the token's smallest unit — test USDC has 7 decimal
places, so `30000000` is 3.0000000 USDC.

```bash
set -a
source deployments/testnet.env
set +a

NETWORK=testnet
ORG_ADDR=$(stellar keys address sci-funder-ms)
CLINIC=G...                       # provider address from the registry or app
SERVICE_CODE=101                  # from the clinic's catalogue
AMOUNT=30000000                   # 3.0000000 USDC
BENEFICIARY_REF=...               # 64 hex characters
EXPIRES_AT=$(( $(date +%s) + 30*86400 ))
```

Build the call without signing it. `--source-account` is the public organisation
address, so this step needs no secret key:

```bash
stellar contract invoke \
  --id "$VOUCHER_CONTRACT_ID" \
  --source-account "$ORG_ADDR" \
  --network "$NETWORK" \
  --build-only \
  -- create_voucher \
  --funder "$ORG_ADDR" \
  --beneficiary_ref "$BENEFICIARY_REF" \
  --provider "$CLINIC" \
  --service_code "$SERVICE_CODE" \
  --amount "$AMOUNT" \
  --expires_at "$EXPIRES_AT" > /tmp/voucher-create.build.xdr
```

Simulate to attach the Soroban footprint and the funder's authorisation entry:

```bash
stellar tx simulate /tmp/voucher-create.build.xdr \
  --source-account "$ORG_ADDR" \
  --network "$NETWORK" > /tmp/voucher-create.xdr
```

Both signers decode and check it, then sign in turn:

```bash
stellar tx decode /tmp/voucher-create.xdr       # inspect contract, function and arguments

stellar tx sign /tmp/voucher-create.xdr \
  --sign-with-key sci-funder-signer-1 --network "$NETWORK" > /tmp/voucher-create.1.xdr

stellar tx sign /tmp/voucher-create.1.xdr \
  --sign-with-key sci-funder-signer-2 --network "$NETWORK" > /tmp/voucher-create.2.xdr

stellar tx send /tmp/voucher-create.2.xdr --network "$NETWORK"
```

The send output includes the transaction hash. The new voucher id is in the
`voucher_created` event, or read `next_voucher_id` before funding. Record both
the hash and the id before the next payment. If the transaction fails, nothing
is escrowed; fix the cause and rebuild rather than reusing a stale XDR.

The other funder calls use the same build → simulate → decode → sign twice →
send pattern. Replace the `-- create_voucher ...` arguments with one of these:

```bash
# Reduce an overpayment while the voucher is still Funded.
-- reduce_amount --funder "$ORG_ADDR" --voucher_id 42 --new_amount 30000000

# Redirect a Funded voucher to a different active clinic.
-- redirect --funder "$ORG_ADDR" --voucher_id 42 --new_provider "$NEW_CLINIC"

# Dispute a Claimed voucher, or an Attested one before its deadline.
-- dispute --funder "$ORG_ADDR" --voucher_id 42 --reason_code 1
```

Dispute reason codes are defined in
[How a voucher works](../protocol/lifecycle.md#disputed): `1` the patient was
not seen, `2` care was not delivered as described, `3` a different service was
provided, `4` something else.

## 4. Refunds back to the multisig

A refund is deliberately permissionless. Anyone — the organisation, the clinic,
the app, or the [settlement keeper](../../examples/keeper.ts) — can call
`refund(voucher_id)` once `refundable_at` has passed. The contract pays the full
escrowed amount to the `funder` stored on the voucher, with no protocol fee.
There is no recipient argument, so a caller cannot divert the money.

For an organisation account the destination is therefore always the multisig.
The same is true for:

- the difference returned by `reduce_amount`;
- a dispute resolved in the funder's favour with
  `resolve_dispute(..., refund_funder: true)`;
- a split resolved with `resolve_split` and a `to_provider` below the escrow;
- a voucher whose clinic was suspended before it was claimed, which becomes
  refundable immediately instead of at expiry.

What to check after a refund:

1. The voucher status is `Refunded` and its `funder` is the organisation
   address. Read it with `get_voucher` or `GET /vouchers/:id`.
2. The organisation's USDC balance rose by the full voucher amount. A refund
   takes no fee.
3. The refund transaction hash is recorded against the voucher. A third-party
   keeper may be the transaction source; that is fine and does not change the
   destination.

If a staff member funded from a personal wallet by mistake, the refund goes to
that personal wallet because that is the address recorded on chain. The
organisation cannot redirect it afterwards. Prevent this by funding only from
the multisig and checking the `funder` field before either signer approves.

## 5. Record every voucher

The chain is the source of truth, but it does not know which internal programme,
patient or budget a voucher belongs to: `beneficiary_ref` is deliberately
opaque. Keep a per-voucher record in the organisation's finance system. At
minimum:

| Field | Why |
| --- | --- |
| Internal case or purchase-order id | Links the voucher to the programme and budget |
| Voucher id | The key for every on-chain and API lookup |
| Creation transaction hash | Proof of funding and the network fee paid |
| Funder address | Must equal the organisation multisig; catches personal-wallet mistakes |
| `beneficiary_ref` | The on-chain pseudonym; keep its mapping to the patient in a separate, access-controlled record |
| Clinic address and name | Who is being paid |
| Service code and label | What care was bought |
| Listed price and amount | Checks the amount was not below the catalogue price and explains any overpayment |
| Expected fee and clinic net | From `quote(amount)`; reconcile against `settledNet` and `settledFee` |
| Expiry | When the voucher becomes refundable if unclaimed |
| Approver names and approval reference | Which two people signed, and the ticket that authorised it |
| Current status and last checked time | Catches stale vouchers that need a refund or dispute |
| Settlement or refund transaction hash | Closes the loop in the accounts |
| Dispute reason and resolution | Explains any money routed by the admin instead of the normal flow |

Useful reads:

```bash
# One voucher, including its funder, status and amount.
stellar contract invoke --id "$VOUCHER_CONTRACT_ID" --source-account "$ORG_ADDR" \
  --network "$NETWORK" -- get_voucher --voucher_id 42

# When refund starts to succeed.
stellar contract invoke --id "$VOUCHER_CONTRACT_ID" --source-account "$ORG_ADDR" \
  --network "$NETWORK" -- refundable_at --voucher_id 42

# Fee and clinic net for an amount, before funding.
stellar contract invoke --id "$VOUCHER_CONTRACT_ID" --source-account "$ORG_ADDR" \
  --network "$NETWORK" -- quote --amount 30000000
```

The backend API can list the organisation's vouchers and derive the due fields,
but treat it as an index, not the ledger. Query it by funder:

```bash
API_URL=${API_URL:-http://localhost:8080}
curl -s "$API_URL/vouchers?funder=$ORG_ADDR&limit=100" | jq .
```

`GET /vouchers` needs at least one filter. Each voucher includes `isRefundable`,
`isSettleable` and `refundableAt`; the API reference explains the fields. Check
the chain before acting on a stale index.

### Reconciliation

At the end of each week or month:

1. List every voucher with `funder=$ORG_ADDR`, including `Settled` and
   `Refunded` ones.
2. Match each id to the internal record and to a creation transaction.
3. For `Refunded` vouchers, confirm the full amount reached the multisig and
   that no fee was deducted.
4. For `Settled` vouchers, compare the recorded `settledNet` and `settledFee`
   with `quote(amount)` at funding time.
5. For open vouchers, check `refundableAt`. Hand due refunds to the keeper or
   trigger them; a permissionless refund needs no signature, so this is safe to
   automate.
6. Investigate any voucher whose `funder` is not the multisig, and any balance
   change not explained by a voucher, refund, reduction or fee.

## 6. Testnet walkthrough

Before the account holds real USDC, run this checklist with a clinic and an
attester on testnet. Use a disposable organisation account and a test
`beneficiary_ref`; never use real patient data.

1. Create the 2-of-3 account, prove that two signers can act, prove that one
   cannot, then disable the master key.
2. Add the USDC trustline, drip test USDC, and confirm the organisation's
   balance in a wallet or on Horizon.
3. Fund a voucher with two signatures. Confirm its `funder` is the organisation
   address and its status is `Funded`.
4. Overfund a second voucher, then call `reduce_amount` with two signatures.
   Confirm the difference arrives back at the multisig and no fee is taken.
5. Have the clinic claim and the attester attest the first voucher, then dispute
   it with two signatures before the deadline. Have the admin resolve it in the
   funder's favour and confirm the refund reaches the multisig. On a separate
   voucher, let the attestation stand and let anyone settle it, confirming the
   clinic is paid.
6. Create a third voucher with a short expiry and leave it unclaimed. Call
   `refund` from an account that is not a signer. Confirm the full amount
   reaches the multisig, proving the permissionless path cannot be diverted.

Only after all six steps pass should the organisation move to a funded account.

## 7. Operating rules

- **Separate duties.** The person who prepares a payment must not be the only
  person who approves it. With 2-of-3, at least one other signer must check the
  decoded transaction.
- **Set a per-voucher and a daily limit** in policy. The protocol can cap what
  one clinic holds in escrow and it caps its own fee, but it does not know the
  organisation's budget. Enforce that before building the transaction.
- **Fund only active clinics and listed services.** `create_voucher` checks this
  on chain, but checking first avoids a failed transaction and a confusing
  error.
- **Do not use the protocol admin account as the funder.** A mistake in one role
  should not be able to affect the other.
- **Keep signer keys off the machine that builds transactions.** Build and
  simulate on an online machine; sign on separate devices; send from either
  signer's machine.
- **Rehearse a lost-signer recovery** before real funds are involved. Test that
  the remaining two signers can remove and replace the third.
- **Never put patient data on chain or in a memo.** The beneficiary reference
  is pseudonymous; the mapping belongs in the organisation's restricted
  records.

For the contract-level details behind these rules, see the
[voucher reference](../developers/contracts.md#voucher), the
[lifecycle](../protocol/lifecycle.md) and the
[trust model](../protocol/trust.md). For the admin-side multisig, see
[Using a multisig account as admin](../developers/multisig-admin.md).
