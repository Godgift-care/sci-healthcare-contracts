---
title: API reference
nav_order: 11
---

# API reference

Base URL in local development: `http://localhost:8080`

The API is **read-only**. It holds no keys and signs nothing. Every state change is a transaction signed in the user's wallet and submitted straight to Soroban RPC.

## Amounts

Every amount is `i128` on chain and is returned as a **decimal string** in the smallest unit — 7 decimal places for USDC.

```
"30000000"  ->  $3.00
```

Do not parse these into JavaScript numbers. Above 2^53 you silently lose precision. Use `BigInt`.

Endpoints that return an amount also return a `…Display` variant already formatted.

---

## `GET /health`

Liveness. Always `{"status":"ok"}` if the process is up.

## `GET /ready`

Readiness, including how far the indexer trails the chain.

```json
{
  "status": "ready",
  "checks": {
    "database": "ok",
    "rpc": "ok",
    "latestLedger": 4466016,
    "indexedLedger": 4466014,
    "lagLedgers": 2,
    "indexer": "ok",
    "network": "testnet"
  }
}
```

Returns **503** when the database is unreachable, RPC is unreachable, or the indexer is more than 120 ledgers behind (roughly 10 minutes). Use this for your health check, not `/health`.

## `GET /stats`

```json
{
  "providers": 1,
  "activeProviders": 1,
  "vouchers": 2,
  "settledVouchers": 1,
  "settledValue": "30000000",
  "receipts": 1
}
```

## `GET /providers`

| Parameter | Type | Notes |
| --- | --- | --- |
| `status` | `Pending` \| `Active` \| `Suspended` | |
| `country` | string(2) | ISO 3166-1 alpha-2 |
| `q` | string | Name search |
| `limit` | int | 1–100, default 50 |
| `offset` | int | default 0 |

```json
{
  "total": 1,
  "limit": 50,
  "offset": 0,
  "providers": [
    {
      "address": "GDOOCNK2HL6TB2Y7FDYNNMG4GTM2PNPY4XCTWG2INFPYYVA66FCPZKBK",
      "name": "Ikeja General Clinic",
      "country": "NG",
      "status": "Active",
      "registeredAt": "2026-09-02T12:36:37.000Z",
      "services": [
        {
          "code": 101,
          "label": "Outpatient consult",
          "price": "30000000",
          "priceDisplay": "3.0000000"
        }
      ]
    }
  ]
}
```

## `GET /providers/:address`

One provider with its full catalogue, including inactive services. **404** if unknown.

## `GET /vouchers`

**Requires at least one filter.** An unfiltered dump is not a useful endpoint and invites scraping.

| Parameter | Type |
| --- | --- |
| `funder` | Stellar address |
| `provider` | Stellar address |
| `beneficiaryRef` | 64 hex characters |
| `status` | `Funded` \| `Claimed` \| `Attested` \| `Settled` \| `Disputed` \| `Refunded` |

Without one, returns **400**:

```json
{
  "error": "filter_required",
  "message": "Provide at least one of: funder, provider, beneficiaryRef, status"
}
```

Each voucher carries derived fields so clients do not re-implement the state machine:

- `serviceLabel` — the clinic's catalogue name for `serviceCode`, or `null` if the service was never indexed
- `refundableAt` — when `refund` starts to succeed: the expiry for a `Funded` voucher, expiry plus the seven-day claim grace for a `Claimed` one, otherwise `null`
- `disputeReason` — the funder's `reason_code` for a disputed voucher (see [the lifecycle](../protocol/lifecycle.md)), otherwise `null`
- `isSettleable` — attested and past the dispute deadline
- `isRefundable` — `refundableAt` has passed

```json
{
  "id": "1",
  "funder": "GDUPJTF3PNSYJ73WWLNTYLG6UXN7HHBKCGFEEMDFMWKWGR3UMQT3JG45",
  "beneficiaryRef": "72676a6f4fff92b09ab1c6368672b05112062f683014d07c9518d4141d094745",
  "provider": { "address": "GDOO…ZKBK", "name": "Ikeja General Clinic", "country": "NG" },
  "serviceCode": 101,
  "serviceLabel": "Outpatient consult",
  "amount": "30000000",
  "amountDisplay": "3.0000000",
  "status": "Settled",
  "createdAt": "2026-09-02T12:37:17.000Z",
  "expiresAt": "2026-10-02T12:37:12.000Z",
  "claimedAt": "2026-09-02T12:37:27.000Z",
  "attestedAt": "2026-09-02T12:37:32.000Z",
  "disputeDeadline": "2026-09-02T12:38:32.000Z",
  "disputeReason": null,
  "settledNet": "29700000",
  "settledFee": "300000",
  "refundableAt": null,
  "isSettleable": false,
  "isRefundable": false
}
```

## `GET /vouchers/:id`

One voucher plus its `receipt` (null unless settled). **404** if unknown.

## `GET /receipts?beneficiaryRef=`

A patient's settled care history. `beneficiaryRef` is **required** and must be exactly 64 hex characters.

Because that value is an HMAC under a key held by the patient, this endpoint cannot be walked to enumerate people.

```json
{
  "total": 1,
  "totalSpend": "30000000",
  "totalSpendDisplay": "3.0000000",
  "receipts": [
    {
      "voucherId": "1",
      "providerAddress": "GDOO…ZKBK",
      "serviceCode": 101,
      "serviceLabel": "Outpatient consult",
      "amount": "30000000",
      "amountDisplay": "3.0000000",
      "settledAt": "2026-09-02T12:38:42.000Z"
    }
  ]
}
```

## Rate limiting

120 requests per minute per IP. Exceeding it returns **429**.

## Errors

| Status | Meaning |
| --- | --- |
| 400 | Invalid or missing query parameters |
| 404 | Not found |
| 429 | Rate limited |
| 503 | Degraded — check `/ready` |
