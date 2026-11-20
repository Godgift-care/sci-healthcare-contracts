---
title: Reading contract events
nav_order: 11
---

# Reading contract events

Every state change worth indexing publishes a typed contract event. This page
is the complete schema, so an app can run its own indexer without reading the
Rust source or the protocol backend.

The source of truth is `contracts/*/src/events.rs`. The test suite pins every
event's topic list and data map against the decoded XDR
(`*_event_snapshot` in each contract's `src/test.rs`), and the backend decoder
in `sci-healthcare-backend/src/stellar/events.ts` is kept in step with it.

## How a Soroban event is shaped

A contract event has three parts:

| Part | Meaning here |
| --- | --- |
| `contractId` | The contract that emitted it. Filter on this first. |
| `topic` | An ordered list of `ScVal`s. `topic[0]` is the event name, then the fields marked `#[topic]` in the Rust struct, in declaration order. |
| `value` | An `ScVal::Map` keyed by field name for the fields not marked `#[topic]`. Events with only topic fields carry an empty map (`{}`), not `undefined`. |

A field listed under **Topics** is **not** in the data map. An indexer that
reads `provider` off the data map for `voucher_created` gets `undefined`; that
is the most common integration mistake.

XDR values decode with the Stellar JS SDK's `scValToNative`:

| Rust type | XDR | JavaScript after `scValToNative` |
| --- | --- | --- |
| `Address` | `ScVal::Address` | `string` (`G…` for an account, `C…` for a contract) |
| `BytesN<32>`, `Bytes` | `ScVal::Bytes` | `Buffer` |
| `String` | `ScVal::String` | `string` |
| `Symbol` | `ScVal::Symbol` | `string` |
| `u32` | `ScVal::U32` | `number` |
| `u64` | `ScVal::U64` | `bigint` |
| `i128` | `ScVal::I128` | `bigint` |
| `bool` | `ScVal::Bool` | `boolean` |
| unit enum (`ProviderStatus`) | `ScVal::U32` | `number` |

Amounts are `i128` in the settlement token's smallest unit. USDC on Stellar has
seven decimals, so `30_000_000n` is 3.00 USDC. Timestamps are `u64` seconds since
the Unix epoch. `beneficiary_ref` is an opaque 32-byte salted hash, not a name
or an identifier that can be reversed on chain.

## Fetching and decoding

`@stellar/stellar-sdk` v13 or later:

```js
import { rpc, scValToNative } from '@stellar/stellar-sdk';

const server = new rpc.Server('https://soroban-testnet.stellar.org');
const latest = await server.getLatestLedger();

const { events } = await server.getEvents({
  startLedger: latest.sequence - 1000,
  filters: [
    {
      type: 'contract',
      contractIds: [VOUCHER_CONTRACT_ID],
    },
  ],
  limit: 100,
});

for (const event of events) {
  // topic[0] is the event name; the rest are indexed fields in order.
  const [name, ...indexed] = event.topic.map((topic) => scValToNative(topic));
  // value is a map keyed by field name, so read fields by name.
  const data = scValToNative(event.value);

  console.log(name, indexed, data);
}
```

`startLedger` must be within the RPC's event retention window. Page with
`cursor` and `pagination.limit`; `getEvents` returns at most `limit` events per
call. To index from the beginning, start at the ledger in which the contract
was deployed.

### Example: a decoded `voucher_created`

```json
{
  "name": "voucher_created",
  "indexed": [
    "GB7F…FUNDER",
    "GCLI…PROVIDER",
    { "type": "Buffer", "data": [9, 9, 9, "…32 bytes…"] }
  ],
  "data": {
    "voucher_id": "1",
    "service_code": 101,
    "amount": "30000000",
    "created_at": "1700000000",
    "expires_at": "1702592000"
  }
}
```

(`bigint` is shown as a string in JSON. In JavaScript the same fields are
`1n`, `30000000n`, and so on.)

### Example: a decoded `voucher_settled`

```json
{
  "name": "voucher_settled",
  "indexed": ["GCLI…PROVIDER"],
  "data": {
    "voucher_id": "1",
    "net": "29700000",
    "fee": "300000",
    "service_code": 101,
    "amount": "30000000",
    "attester_share": "150000"
  }
}
```

## registry

`provider_registered` is emitted by `register_provider`; the rest match their
admin or provider calls.

| Event | Topics (after the name) | Data fields | Emitted by |
| --- | --- | --- | --- |
| `initialized` | — | `admin: Address` | `initialize` |
| `provider_registered` | `provider: Address` | `name: String`, `country: String`, `registered_at: u64` | `register_provider` |
| `provider_status_changed` | `provider: Address` | `status: ProviderStatus` (`0` pending, `1` active, `2` suspended) | `set_provider_status` |
| `service_upserted` | `provider: Address`, `code: u32` | `label: String`, `price: i128` | `upsert_service` |
| `service_removed` | `provider: Address` | `code: u32` | `remove_service` |
| `service_price_scheduled` | `provider: Address`, `code: u32` | `price: i128`, `effective_at: u64` | `schedule_service_price` |
| `service_availability_changed` | `provider: Address`, `code: u32` | `available: bool` | `set_service_available` |
| `attester_added` | `attester: Address` | — (empty map) | `add_attester` |
| `attester_removed` | `attester: Address` | — (empty map) | `remove_attester` |
| `admin_proposed` | `admin: Address` | `pending_admin: Address` | `propose_admin` |
| `admin_changed` | — | `new_admin: Address` | `accept_admin` |

Notes:

- `service_upserted` publishes the price that was written. A raise cannot be
  applied here; it is scheduled and later appears as
  `service_price_scheduled`, then takes effect on reads at `effective_at`.
- `provider_status_changed` carries the enum as a `u32`, so compare against
  `0`, `1`, `2` rather than a string.
- `admin_proposed` is a pending state only. Admin rights change when the
  nominee calls `accept_admin`; `admin_changed` is the event to trust.

## voucher

| Event | Topics (after the name) | Data fields | Emitted by |
| --- | --- | --- | --- |
| `initialized` | — | `admin: Address` | `initialize` |
| `voucher_created` | `funder: Address`, `provider: Address`, `beneficiary_ref: BytesN<32>` | `voucher_id: u64`, `service_code: u32`, `amount: i128`, `created_at: u64`, `expires_at: u64` | `create_voucher`, `create_voucher_at_price` |
| `voucher_claimed` | `provider: Address` | `voucher_id: u64`, `claimed_at: u64`, `service_code: u32`, `amount: i128` | `claim` |
| `voucher_attested` | `attester: Address` | `voucher_id: u64`, `attested_at: u64`, `dispute_deadline: u64`, `service_code: u32`, `amount: i128`, `remaining_today: u32` | `attest` |
| `voucher_disputed` | `funder: Address` | `voucher_id: u64`, `reason_code: u32`, `service_code: u32`, `amount: i128` | `dispute` |
| `voucher_settled` | `provider: Address` | `voucher_id: u64`, `net: i128`, `fee: i128`, `service_code: u32`, `amount: i128`, `attester_share: i128` | `settle`, `resolve_dispute` (provider wins), `resolve_split` with `to_provider > 0` |
| `voucher_refunded` | `funder: Address` | `voucher_id: u64`, `amount: i128`, `service_code: u32` | `refund`, `resolve_dispute` (funder wins), `resolve_split` with `to_provider == 0` |
| `voucher_amount_reduced` | `funder: Address` | `voucher_id: u64`, `service_code: u32`, `previous_amount: i128`, `amount: i128`, `refunded: i128` | `reduce_amount` |
| `dispute_resolved` | `admin: Address` | `voucher_id: u64`, `refunded_funder: bool`, `to_provider: i128`, `fee: i128`, `to_funder: i128` | `resolve_dispute`, `resolve_split` |
| `attestation_cap_changed` | `admin: Address` | `cap: u32` | `set_attestation_cap` |
| `admin_proposed` | `admin: Address` | `pending_admin: Address` | `propose_admin` |
| `admin_changed` | — | `new_admin: Address` | `accept_admin` |

Notes:

- A settlement produces two events for one voucher: `dispute_resolved` and
  then the payment event (`voucher_settled` or `voucher_refunded`). If you
  only need the payout, index the payment event; if you need the reason,
  `dispute_resolved` carries the split.
- `voucher_settled.net + fee == amount`; on a split,
  `dispute_resolved.to_provider + to_funder == voucher.amount` and
  `to_provider - fee` is what the clinic receives.
- `remaining_today` is the attester's allowance after this attestation, not
  before it.
- `voucher_created` and `voucher_claimed`/`voucher_attested`/`voucher_disputed`/
  `voucher_settled`/`voucher_refunded` all carry `service_code` and `amount`, so
  a lifecycle feed does not need a storage read per voucher.
- `voucher_settled.attester_share` is the part of `fee` paid to the attester;
  the fee account receives `fee - attester_share`. The attester share is carved
  out of the fee, so `net + fee == amount` still holds.
- `voucher_amount_reduced` is emitted when a funder takes back an overpayment
  before the clinic claims the voucher. `refunded` is
  `previous_amount - amount`, and the voucher keeps the lower `amount`.

## receipt

| Event | Topics (after the name) | Data fields | Emitted by |
| --- | --- | --- | --- |
| `initialized` | — | `admin: Address` | `initialize` |
| `minter_proposed` | `admin: Address` | `pending_minter: Address` | `propose_minter` |
| `minter_changed` | — | `minter: Address` | `accept_minter` |
| `admin_proposed` | `admin: Address` | `pending_admin: Address` | `propose_admin` |
| `admin_changed` | — | `new_admin: Address` | `accept_admin` |
| `receipt_minted` | `beneficiary_ref: BytesN<32>`, `provider: Address` | `voucher_id: u64`, `service_code: u32`, `amount: i128`, `settled_at: u64` | `mint` (called by the voucher contract on settlement) |

`receipt_minted` is emitted by the receipt book, not the voucher, so filter on
the receipt contract address. `minter_changed` is the authoritative signal that
the voucher contract address changed; `minter_proposed` is a nomination only.

## faucet (testnet only)

| Event | Topics (after the name) | Data fields | Emitted by |
| --- | --- | --- | --- |
| `dripped` | `to: Address` | `amount: i128` | `drip` |
| `drip_changed` | — | `amount: i128`, `cooldown: u64` | `set_drip` |

The faucet has no `initialized` event. It is testnet-only and never holds real
funds.

## Verification

The event schema is checked against the bytes the contracts actually publish:
each `*_event_snapshot` test reads the decoded XDR from the test host and
asserts the exact topic list and data map. To check a live deployment, run the
fetch-and-decode snippet above against the testnet contract addresses in the
[contract reference](contracts.md#building-releasing-and-verifying-the-contracts),
or use Stellar Lab's event viewer for the same contract id and ledger range.

If an event name, a topic position, or a data field changes, the snapshot tests
fail and the change must be coordinated with
`sci-healthcare-backend/src/stellar/events.ts` before release. Adding a field to
the data map is backward compatible for decoders that read by name; moving a
field into or out of the topic list is not.
