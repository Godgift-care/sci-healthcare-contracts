---
title: Trying it by hand in Stellar Lab
nav_order: 12
---

# Trying the protocol by hand in Stellar Lab

[Stellar Lab](https://lab.stellar.org) builds, signs and submits Stellar
transactions from a web page. This walkthrough uses it to run one voucher
through its whole life on testnet: **fund → claim → attest → settle**. Nothing
is installed, which makes it a good fit for workshops.

The other walkthroughs do the same journey with the CLI
([`scripts/seed.sh`](https://github.com/Godgift-care/sci-healthcare-contracts/blob/main/scripts/seed.sh))
or the TypeScript SDK
([`examples/lifecycle.ts`](https://github.com/Godgift-care/sci-healthcare-contracts/blob/main/examples/lifecycle.ts),
[Integrating from another app](integration.md)). Here you make every call
yourself, one form at a time.

> **Testnet only.** In this walkthrough Lab signs with secret keys you paste
> into the page. Generate fresh testnet keys for it. Never paste a key that
> holds real funds.

## Who does what

A voucher involves four parties, and each call must be signed by the right
one:

| Role | Signs |
| --- | --- |
| **Funder** | trustline, `drip`, `create_voucher` |
| **Clinic** | trustline, `register_provider`, `upsert_service`, `claim` |
| **Attester** | trustline, `attest` |
| **Admin** | `set_provider_status` (activates the clinic), `add_attester` |

`settle` is permissionless: any account can sign it.

You create the funder, clinic and attester accounts in Lab. The admin is
whoever deployed the contracts, and the published testnet deployment's admin
key is held by the maintainer. So someone has to run a deployment whose admin
can activate your clinic and add your attester:

- **In a workshop**, the facilitator does it once for the room. See
  [For facilitators](#for-facilitators) below.
- **On your own**, be your own facilitator: the setup runs in a GitHub
  Codespace, which is a browser tab. Do [For facilitators](#for-facilitators)
  first, then come back here.

Whoever does the setup gives you four values. Keep them in a note:

| Value | Example shape |
| --- | --- |
| Registry contract | `C...` (56 characters) |
| Voucher contract | `C...` |
| Faucet contract | `C...` |
| Test USDC issuer | `G...` |

They also tell you the **dispute window**: how long, in seconds, the funder
has to object after an attestation. A workshop deployment uses a short one,
such as 120 seconds, so you can settle the same day.

## How Lab is used on this page

Every step below is one of three things, and they repeat.

**Make an account.** Open **Account → Create account**, generate a keypair
and copy both keys. Then fund it on testnet with Friendbot, either from the
button on that page or from **Account → Fund account**. Friendbot gives you
10,000 test XLM, which pays every fee on this page many times over.

**Call a contract (writes).** Open **Build transaction**, set the source
account to the role that must sign, and add a Soroban **Invoke contract
function** operation. Paste the contract id, and Lab reads the contract's
interface and gives you a form for the function's arguments. Then:

1. **Simulate** (Lab may call this *prepare*): the RPC server works out which
   storage the call touches and the resource fee. If the call would fail, it
   fails here, with the contract's error number, and nothing is sent.
2. **Sign** with the secret key of the source account.
3. **Submit**, then check that the result is `SUCCESS`.

The source account's signature also authorises the call (`require_auth`) when
the address argument is the same account, which is the case for every call
on this page. That is why the role that must sign is always the source
account.

**Read a contract (views).** Read-only functions such as `get_voucher` need
no signature: build the same invoke operation and stop after **Simulate**.
The simulation shows the return value. You can also inspect a contract under
**Smart contracts → Contract explorer**.

Lab's menus move from time to time. If a label here has changed, look for the
same action by name. The steps themselves stay the same.

**Units.** Test USDC has 7 decimals, like real USDC on Stellar, so contract
amounts are in units of 0.0000001: `30000000` is 3.00 USDC.

## 1. Create the three accounts

In Lab, switch the network (top right) to **Testnet**.

Create and Friendbot-fund three accounts, as above: **Funder**, **Clinic** and
**Attester**. In a workshop, a pair or a group of three can split the roles.
Write each public key (`G...`) and secret key (`S...`) in your note.

## 2. Trust test USDC, from all three accounts

Test USDC is an issued asset, and a Stellar account must opt in with a
trustline before it can hold one. The funder needs it to pay. The clinic and
the attester need it to be paid at settlement: without it `settle` fails with
`TokenTransferFailed` (#24).

For each of the three accounts: **Build transaction**, source account = that
account, add a classic **Change trust** operation with asset code `USDC` and
the issuer from your note, leaving the limit empty. Sign with that account's
secret and submit.

## 3. Get test USDC for the funder

Invoke the **faucet** contract:

| Field | Value |
| --- | --- |
| Source account | Funder |
| Function | `drip` |
| `to` | Funder's `G...` |

Sign as the funder and submit. The funder now holds the drip amount, 50 USDC
by default, once a day per address.

## 4. Register the clinic

Invoke the **registry** contract:

| Field | Value |
| --- | --- |
| Source account | Clinic |
| Function | `register_provider` |
| `owner` | Clinic's `G...` |
| `name` | any name, e.g. `Workshop Clinic 7` |
| `country` | two uppercase letters, e.g. `NG` |

The clinic is now `Pending`. A pending clinic cannot list services or receive
vouchers.

**Send the clinic's and the attester's public keys to the facilitator.** They
activate the clinic and add the attester. Until then the next step fails with
`ProviderNotActive`.

## 5. List a service

Once the facilitator confirms, invoke the **registry** again:

| Field | Value |
| --- | --- |
| Source account | Clinic |
| Function | `upsert_service` |
| `provider_addr` | Clinic's `G...` |
| `code` | `101` |
| `label` | `Outpatient consult` |
| `price` | `30000000` (3.00 USDC) |

## 6. Fund a voucher (funder)

The beneficiary reference identifies the patient without naming them. Apps
derive it on the patient's device from a card number and a secret key (see
[Privacy](../protocol/privacy.md) and
[Integrating from another app](integration.md)).
For this walkthrough any 32 bytes will do, written as 64 hex characters. This
one is the SHA-256 of the words `workshop patient 1`:

```text
92f81a75fc1d24770f347d02f2b306e86375c6125e74aaa49176436efccfe6f0
```

The expiry is a Unix timestamp in seconds, and must be in the future. To get
one for 24 hours from now, open your browser's developer console (F12) and
run:

```js
Math.floor(Date.now() / 1000) + 86400
```

Invoke the **voucher** contract:

| Field | Value |
| --- | --- |
| Source account | Funder |
| Function | `create_voucher` |
| `funder` | Funder's `G...` |
| `beneficiary_ref` | the 64 hex characters above |
| `provider` | Clinic's `G...` |
| `service_code` | `101` |
| `amount` | `30000000`. At least the listed price. |
| `expires_at` | the timestamp from the console |

The simulation's return value is the new **voucher id**. Note it: every step
from here uses it. The transfer of 3 USDC from the funder into escrow happens
inside this call, and the funder's signature covers it.

Common failures: `ServiceNotOffered` (#6, wrong clinic or code),
`AmountBelowPrice` (#7) and `InvalidExpiry` (#9, a timestamp in the past or
in milliseconds).

## 7. Claim it (clinic)

The patient turns up and the clinic takes the voucher. Invoke the **voucher**
contract:

| Field | Value |
| --- | --- |
| Source account | Clinic |
| Function | `claim` |
| `provider` | Clinic's `G...` |
| `voucher_id` | your voucher id |

`NotAuthorized` (#3) means the voucher was funded for a different clinic.

## 8. Attest delivery (attester)

Someone other than the clinic confirms that the care happened. Invoke the
**voucher** contract:

| Field | Value |
| --- | --- |
| Source account | Attester |
| Function | `attest` |
| `attester` | Attester's `G...` |
| `voucher_id` | your voucher id |

`NotAuthorized` (#3) here means the facilitator has not added this attester
yet, or you signed with the clinic's account. The contract refuses an
attestation from the clinic being paid, whatever roles it holds.

The dispute window opens now. Until it closes, the funder can still `dispute`
the voucher and an admin decides who gets the money.

## 9. Check the voucher

Simulate `get_voucher` on the **voucher** contract with your `voucher_id`
(any source account, no signature). Two fields matter:

- `status` should be `2` (`Attested`). The values are 0 `Funded`,
  1 `Claimed`, 2 `Attested`, 3 `Settled`, 4 `Disputed`, 5 `Refunded`.
- `dispute_deadline` is the Unix time the window closes. Compare it with
  `Math.floor(Date.now() / 1000)` in the console.

## 10. Settle (anyone)

Once the dispute deadline has passed, invoke the **voucher** contract from
any of your accounts:

| Field | Value |
| --- | --- |
| Source account | any of the three |
| Function | `settle` |
| `voucher_id` | your voucher id |

Too early, and the simulation fails with `DisputeWindowOpen` (#13). Wait and
try again.

Settlement splits the 3.00 USDC three ways. With the default deployment
settings (a 1% fee, half of it to the attester):

| Paid to | Amount |
| --- | --- |
| Clinic | `29700000` (2.97 USDC) |
| Attester | `150000` (0.015 USDC) |
| Fee account (the admin) | `150000` (0.015 USDC) |

The three always add up to the voucher amount exactly.

## 11. See the result

- `get_voucher` now shows `status` `3` (`Settled`).
- The clinic's and the attester's USDC balances in Lab (**Account**, or
  [stellar.expert](https://stellar.expert/explorer/testnet)) show the payouts.
- Settlement minted a care receipt. The receipt book's address is in the
  voucher contract's `get_config` (field `receipt_book`). Simulate
  `get_receipt` on it with your voucher id.

That is the whole protocol: money bound to one clinic and one service,
released only after someone other than the clinic confirmed the care, and
paid out by a call anyone could make.

## Where to go next

- Run the other endings. A voucher that is never claimed can be `refund`ed
  once it expires. Calling `dispute` (signed by the funder) between step 8
  and step 10 moves the voucher to `Disputed`, and the admin settles it with
  `resolve_dispute`. [How a voucher works](../protocol/lifecycle.md) has
  every path.
- [Contract reference](contracts.md) lists every function, error and event.
- [Integrating from another app](integration.md) does the same calls from
  code.

## For facilitators

The setup needs the Stellar CLI and `wasm-opt`. The repository's dev
container has both, so a GitHub Codespace works from a browser with nothing
installed.

1. On the repository page choose **Code → Codespaces → Create codespace on
   main**. The first start builds the container and fetches dependencies.
2. In the codespace terminal, create and fund the admin and issuer
   identities:

   ```bash
   stellar keys generate sci-admin --network testnet --fund
   stellar keys generate sci-issuer --network testnet --fund
   ```

3. Deploy your own copy with a short dispute window, then the faucet.
   `deployments/testnet.env` in the repository describes the published
   deployment, so `deploy.sh` refuses to replace it without `--force`. Inside
   the codespace that is what you want, but do not commit the file it
   writes.

   ```bash
   DISPUTE_WINDOW=120 ./scripts/deploy.sh testnet --force
   DAILY_CAP=100000000000 ./scripts/deploy-faucet.sh testnet
   ```

   The faucet's default daily cap, 250 USDC across all addresses, runs out
   after five participants. `DAILY_CAP` above raises it to 10,000 USDC.

4. Share the registry, voucher and faucet ids and the USDC issuer with the
   room:

   ```bash
   cat deployments/testnet.env
   ```

5. When a participant sends you their clinic and attester keys, activate the
   clinic (`status` `1` is `Active`; reason `1` is "verified") and add the
   attester:

   ```bash
   source deployments/testnet.env
   ADMIN_ADDR="$(stellar keys address sci-admin)"

   stellar contract invoke --id "$REGISTRY_CONTRACT_ID" \
     --source-account sci-admin --network testnet -- \
     set_provider_status --admin "$ADMIN_ADDR" \
     --provider_addr <CLINIC_G_ADDRESS> --status 1 --reason_code 1

   stellar contract invoke --id "$REGISTRY_CONTRACT_ID" \
     --source-account sci-admin --network testnet -- \
     add_attester --admin "$ADMIN_ADDR" --attester <ATTESTER_G_ADDRESS>
   ```

   These are the two calls in the table at the top that need the admin. You
   can also make them in Lab, with the admin's secret from
   `stellar keys show sci-admin`, but on a projector the CLI keeps that
   secret off the screen.

A participant's own attester can attest any clinic's voucher except one it is
the clinic for. One attester per table is enough if time is short.
