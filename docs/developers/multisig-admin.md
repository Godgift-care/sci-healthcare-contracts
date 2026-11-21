---
title: Using a multisig account as admin
nav_order: 12
---

# Using a multisig account as admin

A single key controlling provider verification, attester appointments and
dispute resolution is a single point of failure. On mainnet the admin of all
three contracts should be a Stellar account with a **2-of-3 multisig** policy,
so no one person can suspend a clinic or route escrowed funds alone.

This guide creates that account on testnet, hands it the admin role through the
two-step handover, and shows how to submit an admin call with two signatures.

This is the **admin** account. An organisation that funds vouchers from a shared
wallet needs a separate funder multisig, not the admin account; see
[Funding from an organisation wallet](../guides/organisation-funders.md).

## What the contracts see

Each contract stores one `Address` as admin and checks it before calling
`require_auth()`. For a Stellar account (`G...`) that address is the account,
not its individual signers. The Stellar network enforces the account's signer
weights and thresholds before the contract is invoked, so the contract does not
need to know how many signatures were used.

That also means:

- A multisig account behaves exactly like any other admin address.
- Losing one signer does not lock the account; losing two does.
- Handing admin to a multisig is reversible only with the multisig's approval
  (or the old admin, before the handover completes).
- The account's **master key** must be disabled after the signers are added, or
  it can bypass the 2-of-3 policy.

## 1. Generate the account and three signers

For this walkthrough the account is `sci-admin-ms` and the signers are
`sci-admin-signer-1`, `sci-admin-signer-2` and `sci-admin-signer-3`. In
production each signer must live with a different person, on separate hardware,
in a different place. Do not store all three on one machine.

```bash
NETWORK=testnet

stellar keys generate --network "$NETWORK" --fund sci-admin-ms
stellar keys generate --network "$NETWORK" --fund sci-admin-signer-1
stellar keys generate --network "$NETWORK" --fund sci-admin-signer-2
stellar keys generate --network "$NETWORK" --fund sci-admin-signer-3
```

Record the public addresses:

```bash
MS=$(stellar keys address sci-admin-ms)
S1=$(stellar keys address sci-admin-signer-1)
S2=$(stellar keys address sci-admin-signer-2)
S3=$(stellar keys address sci-admin-signer-3)

printf 'multisig %s\nsigner 1 %s\nsigner 2 %s\nsigner 3 %s\n' "$MS" "$S1" "$S2" "$S3"
```

## 2. Add the three signers

The Stellar CLI's `tx new set-options` accepts **one signer per transaction**,
so add them one at a time. Each signer gets weight `1`. For a 2-of-3 policy,
any two weight-1 signatures reach the threshold.

```bash
for signer in sci-admin-signer-1 sci-admin-signer-2 sci-admin-signer-3; do
  address=$(stellar keys address "$signer")
  stellar tx new set-options \
    --source-account sci-admin-ms \
    --network "$NETWORK" \
    --signer "$address" \
    --signer-weight 1
done
```

At this point the master key still has weight `1`. Check the account:

```bash
curl -s "https://horizon-testnet.stellar.org/accounts/$MS" \
  | jq '{signers: [.signers[] | {key, weight}], thresholds}'
```

You should see four entries: the account's master key with weight `1` and the
three new signers with weight `1`. Horizon reports the master key in the
`signers` array, so its weight is what you change in step 4.

## 3. Require two signatures

Set all three thresholds to `2`, but leave the master weight at `1` for now.
The transaction is still authorised under the old thresholds, so the master key
can sign it.

```bash
stellar tx new set-options \
  --source-account sci-admin-ms \
  --network "$NETWORK" \
  --low-threshold 2 \
  --med-threshold 2 \
  --high-threshold 2
```

From now on a single key cannot authorise a transaction.

### Test that 2-of-3 works before you disable the master key

Build a no-op transaction (it sets the master weight to the value it already
has), sign it with signer 1, then signer 2, then send it:

```bash
stellar tx new set-options \
  --source-account "$MS" \
  --network "$NETWORK" \
  --master-weight 1 \
  --build-only > /tmp/ms-test.xdr

stellar tx sign /tmp/ms-test.xdr \
  --sign-with-key sci-admin-signer-1 --network "$NETWORK" > /tmp/ms-test.1.xdr

stellar tx sign /tmp/ms-test.1.xdr \
  --sign-with-key sci-admin-signer-2 --network "$NETWORK" > /tmp/ms-test.2.xdr

stellar tx send /tmp/ms-test.2.xdr --network "$NETWORK"
```

The same flow with only one signer must fail with a signature/threshold error.
Record both outcomes before continuing.

## 4. Disable the master key

This is the step that makes the account genuinely 2-of-3. Do it only after the
test above succeeds. The transaction needs two signer keys to send, because the
current thresholds are already `2`.

```bash
stellar tx new set-options \
  --source-account "$MS" \
  --network "$NETWORK" \
  --master-weight 0 \
  --build-only > /tmp/ms-disable-master.xdr

stellar tx sign /tmp/ms-disable-master.xdr \
  --sign-with-key sci-admin-signer-1 --network "$NETWORK" > /tmp/ms-disable-master.1.xdr

stellar tx sign /tmp/ms-disable-master.1.xdr \
  --sign-with-key sci-admin-signer-2 --network "$NETWORK" > /tmp/ms-disable-master.2.xdr

stellar tx send /tmp/ms-disable-master.2.xdr --network "$NETWORK"
```

Check the result:

```bash
curl -s "https://horizon-testnet.stellar.org/accounts/$MS" \
  | jq '{thresholds, signers: [.signers[] | {key, weight}]}'
```

All thresholds should be `2`, the three signers should have weight `1`, and the
account's master key should appear with weight `0` (or be absent, depending on
the Horizon version), so it can no longer sign.

> **Do not skip the test in step 3.** If you disable the master key before
> proving two signers can act, and one signer is misconfigured, the account is
> permanently locked.

## 5. Hand the contracts to the multisig

Deploy with a temporary funded deployer identity (for example `sci-admin`), not
with the multisig account: `scripts/deploy.sh` needs a local key to sign the
deployment transactions. Then hand admin to the multisig.

```bash
# Read the addresses written by scripts/deploy.sh.
set -a
source deployments/testnet.env
set +a

DEPLOYER_ID=sci-admin
DEPLOYER=$(stellar keys address "$DEPLOYER_ID")

for contract in \
  "$REGISTRY_CONTRACT_ID" \
  "$VOUCHER_CONTRACT_ID" \
  "$RECEIPT_CONTRACT_ID"
do
  stellar contract invoke \
    --id "$contract" \
    --source-account "$DEPLOYER_ID" \
    --network "$NETWORK" \
    -- propose_admin \
    --admin "$DEPLOYER" \
    --new_admin "$MS"
done
```

Nothing changes until the nominee accepts. Build each `accept_admin` with the
multisig account as the source, simulate it to attach the Soroban footprint and
authorization entries, then add two signatures:

```bash
for contract in \
  "$REGISTRY_CONTRACT_ID" \
  "$VOUCHER_CONTRACT_ID" \
  "$RECEIPT_CONTRACT_ID"
do
  # Build only: no simulation, no signatures, no network submission.
  stellar contract invoke \
    --id "$contract" \
    --source-account "$MS" \
    --network "$NETWORK" \
    --build-only \
    -- accept_admin \
    --new_admin "$MS" > "/tmp/accept-$contract.build.xdr"

  # Simulate: adds the footprint and the source-account auth entry.
  stellar tx simulate "/tmp/accept-$contract.build.xdr" \
    --source-account "$MS" \
    --network "$NETWORK" > "/tmp/accept-$contract.xdr"

  stellar tx sign "/tmp/accept-$contract.xdr" \
    --sign-with-key sci-admin-signer-1 \
    --network "$NETWORK" > "/tmp/accept-$contract.1.xdr"

  stellar tx sign "/tmp/accept-$contract.1.xdr" \
    --sign-with-key sci-admin-signer-2 \
    --network "$NETWORK" > "/tmp/accept-$contract.2.xdr"

  stellar tx send "/tmp/accept-$contract.2.xdr" --network "$NETWORK"
done
```

`--build-only` deliberately skips simulation, so the unsigned transaction has
no footprint and no auth entry. Sending it directly fails. `stellar tx simulate`
fills both in; because the transaction source is the multisig account, the auth
entry uses `source_account` credentials and the two envelope signatures are what
satisfy the 2-of-3 threshold.

## 6. Verify the handover

Read the admin back from each contract:

```bash
stellar contract invoke --id "$REGISTRY_CONTRACT_ID" --source-account "$MS" \
  --network "$NETWORK" -- get_admin
stellar contract invoke --id "$VOUCHER_CONTRACT_ID" --source-account "$MS" \
  --network "$NETWORK" -- get_config
stellar contract invoke --id "$RECEIPT_CONTRACT_ID" --source-account "$MS" \
  --network "$NETWORK" -- get_admin
```

All three must return the multisig address `$MS`. Each contract should also
return `None` for `get_pending_admin`:

```bash
for contract in \
  "$REGISTRY_CONTRACT_ID" \
  "$VOUCHER_CONTRACT_ID" \
  "$RECEIPT_CONTRACT_ID"
do
  stellar contract invoke --id "$contract" --source-account "$MS" \
    --network "$NETWORK" -- get_pending_admin
done
```

Prove the old deployer key is powerless:

```bash
# This must fail with NotAuthorized (#3).
stellar contract invoke \
  --id "$REGISTRY_CONTRACT_ID" \
  --source-account "$DEPLOYER_ID" \
  --network "$NETWORK" \
  -- add_attester --admin "$DEPLOYER" --attester "$(stellar keys address sci-admin-signer-1)"
```

## 7. Make an admin call with the multisig

Every admin call follows the same pattern: build, simulate, sign twice, send.
For example, to add an attester:

```bash
stellar contract invoke \
  --id "$REGISTRY_CONTRACT_ID" \
  --source-account "$MS" \
  --network "$NETWORK" \
  --build-only \
  -- add_attester \
  --admin "$MS" \
  --attester "$(stellar keys address sci-clinic)" > /tmp/add-attester.build.xdr

stellar tx simulate /tmp/add-attester.build.xdr \
  --source-account "$MS" \
  --network "$NETWORK" > /tmp/add-attester.xdr

stellar tx sign /tmp/add-attester.xdr \
  --sign-with-key sci-admin-signer-1 --network "$NETWORK" > /tmp/add-attester.1.xdr

stellar tx sign /tmp/add-attester.1.xdr \
  --sign-with-key sci-admin-signer-2 --network "$NETWORK" > /tmp/add-attester.2.xdr

stellar tx send /tmp/add-attester.2.xdr --network "$NETWORK"
```

The same flow resolves a dispute:

```bash
# Build with the fields the voucher needs.
stellar contract invoke \
  --id "$VOUCHER_CONTRACT_ID" \
  --source-account "$MS" \
  --network "$NETWORK" \
  --build-only \
  -- resolve_dispute \
  --admin "$MS" \
  --voucher_id 1 \
  --refund_funder true > /tmp/resolve.build.xdr

stellar tx simulate /tmp/resolve.build.xdr \
  --source-account "$MS" \
  --network "$NETWORK" > /tmp/resolve.xdr

stellar tx sign /tmp/resolve.xdr \
  --sign-with-key sci-admin-signer-1 --network "$NETWORK" > /tmp/resolve.1.xdr

stellar tx sign /tmp/resolve.1.xdr \
  --sign-with-key sci-admin-signer-2 --network "$NETWORK" > /tmp/resolve.2.xdr

stellar tx send /tmp/resolve.2.xdr --network "$NETWORK"
```

In production the signers do not share a machine. Stop after
`stellar tx simulate`, send the unsigned XDR to signer 1, then pass the
partially signed XDR to signer 2, and let either of them submit it. The last
signer can also submit, but the two signatures are what matter.

## Rotating or removing a signer

Once the master key is disabled, any change to signers or thresholds needs two
existing signatures. Remove a signer by setting their weight to `0`:

```bash
stellar tx new set-options \
  --source-account "$MS" \
  --network "$NETWORK" \
  --signer "$S3" \
  --signer-weight 0 \
  --build-only > /tmp/remove-signer.xdr

stellar tx sign /tmp/remove-signer.xdr \
  --sign-with-key sci-admin-signer-1 --network "$NETWORK" > /tmp/remove-signer.1.xdr

stellar tx sign /tmp/remove-signer.1.xdr \
  --sign-with-key sci-admin-signer-2 --network "$NETWORK" > /tmp/remove-signer.2.xdr

stellar tx send /tmp/remove-signer.2.xdr --network "$NETWORK"
```

Add a replacement the same way, with weight `1`. Re-check the Horizon account
record after every change and record the transaction hashes in the mainnet
launch issue.

## If you lose a signer

With 2-of-3, losing one signer leaves the account usable: the remaining two can
remove the lost key and add a replacement. Losing two signers is unrecoverable
unless you still have an off-chain backup. Keep sealed backups, rehearse a
restore, and never let one person hold two of the three shares.

## Contract test

`multisig_admin_can_act_on_all_three_contracts` in
`contracts/voucher/src/test.rs` hands registry, voucher and receipt to one
account, then performs an admin action on each. It proves the contracts accept
that account as admin after the two-step handover. The 2-of-3 signature policy
itself is enforced by Stellar, outside the contract, so it is covered by the
testnet walkthrough above rather than by a unit test.

For the launch gates that depend on this guide, see the
[mainnet launch checklist](mainnet-checklist.md).
