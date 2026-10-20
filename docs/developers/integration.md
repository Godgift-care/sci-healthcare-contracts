---
title: Integrating from another app
nav_order: 11
---

# Integrating with SCI Healthcare from another app

A wallet, remittance app or clinic system can offer care vouchers without using
the SCI Healthcare frontend. This guide uses the official Stellar JavaScript SDK
and talks to the contracts directly.

The flow is:

1. Derive the beneficiary reference on the user's device.
2. Read the clinic's service and quote the fee.
3. Build, simulate, sign and submit `create_voucher`.
4. Read the voucher status from `voucher`.
5. Read the receipt and care history from `receipt`.

The examples run against **testnet**. Mainnet is not open: the contracts are
unaudited. See the [mainnet launch checklist](mainnet-checklist.md).

## Prerequisites

- Node.js 22 or newer.
- A Stellar account with XLM for fees.
- A USDC trustline and enough USDC for the voucher amount.
- The testnet contract addresses. They are also in
  [`deployments/testnet.env`](../../deployments/testnet.env).

```bash
mkdir sci-integration && cd sci-integration
npm init -y
npm pkg set type=module
npm install @stellar/stellar-sdk@17.0.1
```

Set the addresses from your deployment. Do not put secret keys in source code or
commit them.

```bash
export SOROBAN_RPC_URL=https://soroban-testnet.stellar.org
export REGISTRY_CONTRACT_ID=CAMU635NRFEHATJ6MTY5SI43IAZ43KHPFXAQDQAST2AHKCHLSGWHVE4V
export VOUCHER_CONTRACT_ID=CBPUQN6CNJCNGPKNEXB7UM4T7RINKQB7HQI5CXEOSQ5EFRHE4G4VKPSQ
export RECEIPT_CONTRACT_ID=CBCJRI2BCZBMZNX77WWDC2NGSV4ZNAN5Q23XHYNBH6UTOQMPJHZGX5C7
export USDC_CONTRACT_ID=CCRLJ7FEIKTR3GBT3VQQPSIJGRBO2WPNMVFKJDP6AYGJQWECOR4QVE5M
```

The testnet voucher address above is the deployed one in `deployments/testnet.env`;
use the values from your own deployment when they change.

## The client module

Save this as `sci.mjs`. It contains the two things every integration needs:
`read` simulates a view call (free, no signature), and `invoke` builds,
simulates, signs and submits a state-changing call.

```js
// sci.mjs
import { createHmac, randomBytes } from 'node:crypto';
import { setTimeout as sleep } from 'node:timers/promises';
import {
  Account,
  Address,
  BASE_FEE,
  Contract,
  Keypair,
  TransactionBuilder,
  nativeToScVal,
  rpc,
  scValToNative,
  xdr,
} from '@stellar/stellar-sdk';

export const NETWORK_PASSPHRASE =
  process.env.STELLAR_NETWORK_PASSPHRASE ?? 'Test SDF Network ; September 2015';
export const RPC_URL =
  process.env.SOROBAN_RPC_URL ?? 'https://soroban-testnet.stellar.org';

export const contracts = {
  registry: process.env.REGISTRY_CONTRACT_ID,
  voucher: process.env.VOUCHER_CONTRACT_ID,
  receipt: process.env.RECEIPT_CONTRACT_ID,
  usdc: process.env.USDC_CONTRACT_ID,
};

const server = new rpc.Server(RPC_URL, {
  allowHttp: RPC_URL.startsWith('http://'),
});

// A well-formed but unfunded account is enough to simulate a read.
const READER = 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF';

export const arg = {
  address: (value) => Address.fromString(value).toScVal(),
  u32: (value) => nativeToScVal(value, { type: 'u32' }),
  u64: (value) => nativeToScVal(BigInt(value), { type: 'u64' }),
  i128: (value) => nativeToScVal(BigInt(value), { type: 'i128' }),
  string: (value) => nativeToScVal(value, { type: 'string' }),
  bytes32: (value) => {
    const hex = value.startsWith('0x') ? value.slice(2) : value;
    if (!/^[0-9a-fA-F]{64}$/.test(hex)) {
      throw new Error('beneficiary_ref must be 32 bytes (64 hex characters)');
    }
    return xdr.ScVal.scvBytes(Buffer.from(hex, 'hex'));
  },
};

export async function read(contractId, method, args = []) {
  if (!contractId) throw new Error(`No contract id configured for ${method}`);

  const tx = new TransactionBuilder(new Account(READER, '0'), {
    fee: BASE_FEE,
    networkPassphrase: NETWORK_PASSPHRASE,
  })
    .addOperation(new Contract(contractId).call(method, ...args))
    .setTimeout(30)
    .build();

  const simulation = await server.simulateTransaction(tx);
  if (rpc.Api.isSimulationError(simulation)) {
    throw describeError(contractId, method, simulation.error);
  }
  if (!simulation.result?.retval) {
    throw new Error(`${method} returned no value`);
  }
  return scValToNative(simulation.result.retval);
}

export async function invoke(secret, contractId, method, args = []) {
  const signer = Keypair.fromSecret(secret);
  const account = await server.getAccount(signer.publicKey());

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK_PASSPHRASE,
  })
    .addOperation(new Contract(contractId).call(method, ...args))
    .setTimeout(60)
    .build();

  // Simulate before asking the user to sign: a transaction that would fail
  // should fail here, not after it has been approved.
  const simulation = await server.simulateTransaction(tx);
  if (rpc.Api.isSimulationError(simulation)) {
    throw describeError(contractId, method, simulation.error);
  }

  // Simulation supplies the footprint, resource fee and auth entries. Without
  // assembleTransaction the submission fails with a resource error.
  const prepared = rpc.assembleTransaction(tx, simulation).build();
  prepared.sign(signer);

  const sent = await server.sendTransaction(prepared);
  if (sent.status === 'ERROR') {
    throw new Error(`Transaction rejected: ${JSON.stringify(sent.errorResult ?? sent.status)}`);
  }

  const result = await waitForTransaction(sent.hash);
  return {
    hash: sent.hash,
    value: result.returnValue ? scValToNative(result.returnValue) : null,
  };
}

async function waitForTransaction(hash) {
  const deadline = Date.now() + 60_000;
  while (Date.now() < deadline) {
    const result = await server.getTransaction(hash);
    if (result.status === rpc.Api.GetTransactionStatus.SUCCESS) return result;
    if (result.status === rpc.Api.GetTransactionStatus.FAILED) {
      throw new Error(`Transaction ${hash} failed on chain`);
    }
    await sleep(1_500);
  }
  throw new Error(
    `Transaction ${hash} did not confirm in 60s. Check the explorer before resubmitting.`,
  );
}

// Contract errors arrive as "Error(Contract, #N)". The full list for each
// contract is in docs/developers/contracts.md.
const REGISTRY_ERRORS = {
  1: 'AlreadyInitialized',
  2: 'NotInitialized',
  3: 'NotAuthorized',
  4: 'ProviderNotFound',
  5: 'ProviderExists',
  6: 'ProviderNotActive',
  7: 'ServiceNotFound',
  8: 'InvalidPrice',
  9: 'InvalidCountry',
  10: 'EmptyName',
  11: 'NoPendingAdmin',
};

const VOUCHER_ERRORS = {
  1: 'AlreadyInitialized',
  2: 'NotInitialized',
  3: 'NotAuthorized',
  4: 'VoucherNotFound',
  5: 'ProviderNotActive',
  6: 'ServiceNotOffered',
  7: 'AmountBelowPrice',
  8: 'InvalidAmount',
  9: 'InvalidExpiry',
  10: 'InvalidStatus',
  11: 'VoucherExpired',
  12: 'NotYetExpired',
  13: 'DisputeWindowOpen',
  14: 'DisputeWindowClosed',
  15: 'InvalidFee',
  16: 'MathOverflow',
  17: 'NoPendingAdmin',
  18: 'ServiceUnavailable',
  19: 'AttestationCapReached',
  20: 'InvalidAttestationCap',
};

const RECEIPT_ERRORS = {
  1: 'AlreadyInitialized',
  2: 'NotInitialized',
  3: 'NotAuthorized',
  4: 'ReceiptNotFound',
  5: 'ReceiptExists',
  6: 'InvalidAmount',
  7: 'NoPendingAdmin',
};

function describeError(contractId, method, message) {
  const text = String(message ?? '');
  const match = /Error\(Contract,\s*#(\d+)\)/.exec(text);
  if (!match) return new Error(`${method} failed: ${text}`);

  const code = Number(match[1]);
  const names =
    contractId === contracts.receipt
      ? RECEIPT_ERRORS
      : contractId === contracts.registry
        ? REGISTRY_ERRORS
        : VOUCHER_ERRORS;
  const name = names[code] ?? 'ContractError';
  const error = new Error(`${method} failed: ${name} (#${code})`);
  error.code = code;
  error.contractError = name;
  return error;
}
```

### Browser signing instead of a secret key

Wallets should not pass a secret key into this module. Replace the signing part
of `invoke` with the wallet's signing callback:

```js
// after rpc.assembleTransaction(...)
const prepared = rpc.assembleTransaction(tx, simulation).build();
const signedXdr = await wallet.signTransaction(prepared.toXDR(), {
  networkPassphrase: NETWORK_PASSPHRASE,
});
const signed = TransactionBuilder.fromXDR(signedXdr, NETWORK_PASSPHRASE);
const sent = await server.sendTransaction(signed);
```

The rest of the flow is identical.

## 1. Derive the beneficiary reference

The chain must never receive a patient identifier. Derive a 32-byte
HMAC-SHA256 reference **on the user's device**:

```js
// in sci.mjs
export function deriveBeneficiaryRef(identifier, key) {
  const normalized = identifier.trim().toLowerCase();
  if (!normalized) throw new Error('Enter a patient reference');
  if (key.length < 32) throw new Error('The secret key must be at least 32 characters');

  return createHmac('sha256', key).update(normalized).digest();
}

export function generateBeneficiaryKey() {
  return randomBytes(32).toString('base64url');
}
```

```js
import { deriveBeneficiaryRef, generateBeneficiaryKey } from './sci.mjs';

// The user keeps the key. Losing it means losing the ability to look up the
// care history; sending it to a server defeats the privacy design.
const key = generateBeneficiaryKey();
const ref = deriveBeneficiaryRef('clinic-card-12345', key);

console.log('beneficiary_ref', ref.toString('hex'));
console.log('store this key safely', key);
```

Rules:

- Trim and lowercase the identifier **before** hashing. The same identifier and
  key must always produce the same reference.
- Use a different random key for each patient and keep it on the device. The
  reference is pseudonymous, not anonymous: anyone with both the identifier and
  the key can confirm a match.
- Never send the identifier or key to the API, a log, analytics, or the ledger.
- In a browser, use the Web Crypto equivalent:

```js
async function deriveBeneficiaryRefBrowser(identifier, key) {
  const id = identifier.trim().toLowerCase();
  const cryptoKey = await crypto.subtle.importKey(
    'raw',
    new TextEncoder().encode(key),
    { name: 'HMAC', hash: 'SHA-256' },
    false,
    ['sign'],
  );
  const signature = await crypto.subtle.sign(
    'HMAC',
    cryptoKey,
    new TextEncoder().encode(id),
  );
  return new Uint8Array(signature);
}
```

## 2. Read the clinic, service and fee quote

Before showing a price, check that the clinic is active and that the service is
listed. `get_service_price` returns `0` for a missing or inactive service, and
`is_service_available` returns `false` while the clinic has paused it.
`get_upcoming_price` returns `(price, effective_at)` when the clinic has
scheduled a price change (at least 24 hours ahead), or `(0, 0)` — show it so the
funder is not surprised.

The provider below is the clinic created by the current testnet seed; replace it
with the provider from your own deployment.

```js
import { arg, contracts, read } from './sci.mjs';

const provider = 'GBWBYKGC5OCRELVP5WHAKEMBSZMNUY3HOKVSYBQGLFNWWQHNH2SD53FZ';
const serviceCode = 101;
const amount = 30_000_000n; // 3.00 USDC at 7 decimals

const active = await read(contracts.registry, 'is_active_provider', [
  arg.address(provider),
]);
const price = await read(contracts.registry, 'get_service_price', [
  arg.address(provider),
  arg.u32(serviceCode),
]);

if (!active || price === 0n) {
  throw new Error('The clinic is not active or does not offer this service');
}
const available = await read(contracts.registry, 'is_service_available', [
  arg.address(provider),
  arg.u32(serviceCode),
]);
if (!available) {
  throw new Error('The clinic has paused this service for now');
}
if (amount < price) {
  throw new Error(`Amount must be at least ${price} (the listed price)`);
}

const [fee, net] = await read(contracts.voucher, 'quote', [arg.i128(amount)]);
const config = await read(contracts.voucher, 'get_config', []);

console.log({
  listedPrice: price,
  amount,
  feeBps: config.fee_bps,
  fee,
  clinicReceives: net,
});
```

`quote` returns `[fee, net]` as `BigInt`. Fees are basis points, rounded down in
the clinic's favour. Display the quoted clinic amount beside the amount the user
will pay; never calculate it in floating point.

## 3. Fund the voucher

`create_voucher` escrows the money in the voucher contract. The funder can be the
patient or someone else, so a remittance sender uses the same call.

```js
import {
  arg,
  contracts,
  deriveBeneficiaryRef,
  invoke,
} from './sci.mjs';
import { Keypair } from '@stellar/stellar-sdk';

const funderKey = Keypair.fromSecret(process.env.FUNDER_SECRET);
const funder = funderKey.publicKey();
const provider = 'GBWBYKGC5OCRELVP5WHAKEMBSZMNUY3HOKVSYBQGLFNWWQHNH2SD53FZ';
const serviceCode = 101;
const amount = 30_000_000n;
const beneficiaryRef = deriveBeneficiaryRef('clinic-card-12345', process.env.PATIENT_KEY);
const expiresAt = BigInt(Math.floor(Date.now() / 1000) + 30 * 24 * 60 * 60);

const { hash, value: voucherId } = await invoke(
  process.env.FUNDER_SECRET,
  contracts.voucher,
  'create_voucher',
  [
    arg.address(funder),
    arg.bytes32(beneficiaryRef.toString('hex')),
    arg.address(provider),
    arg.u32(serviceCode),
    arg.i128(amount),
    arg.u64(expiresAt),
  ],
);

console.log('funded', { hash, voucherId: String(voucherId) });
```

Before this call succeeds:

- The provider must be `Active` in the registry.
- The service must be listed and active.
- `amount` must be at least the listed price.
- `expires_at` must be in the future, in seconds.
- The funder must hold enough USDC and have a USDC trustline. The contract
  transfers the token before writing the voucher, so a failed transfer reverts
  the whole call.

## 4. Read the voucher status

Use `get_voucher` to show the current state. `status` is the enum discriminant:

```js
import { arg, contracts, read } from './sci.mjs';

const STATUS = ['Funded', 'Claimed', 'Attested', 'Settled', 'Disputed', 'Refunded'];

const voucherId = 1n;
const voucher = await read(contracts.voucher, 'get_voucher', [arg.u64(voucherId)]);

console.log({
  id: String(voucher.id),
  status: STATUS[Number(voucher.status)] ?? String(voucher.status),
  funder: voucher.funder,
  provider: voucher.provider,
  serviceCode: voucher.service_code,
  amount: voucher.amount,
  beneficiaryRef: Buffer.from(voucher.beneficiary_ref).toString('hex'),
  createdAt: new Date(Number(voucher.created_at) * 1000),
  expiresAt: new Date(Number(voucher.expires_at) * 1000),
  claimedAt: voucher.claimed_at ? new Date(Number(voucher.claimed_at) * 1000) : null,
  attestedAt: voucher.attested_at ? new Date(Number(voucher.attested_at) * 1000) : null,
  disputeDeadline: voucher.dispute_deadline
    ? new Date(Number(voucher.dispute_deadline) * 1000)
    : null,
});
```

What to show users:

| Status | Meaning |
| --- | --- |
| `Funded` | Money is escrowed; the clinic has not claimed it yet. |
| `Claimed` | The clinic says the patient attended. |
| `Attested` | An independent attester confirmed delivery; the dispute window is open. |
| `Settled` | The clinic was paid and a receipt was minted. |
| `Disputed` | The funder contested it; the admin decides where the money goes. |
| `Refunded` | The money returned to the funder. |

For a refund, `refundable_at(voucher_id)` returns the ledger time at which
`refund` will succeed. `refund` and `settle` are permissionless, so either party
can submit them when eligible.

## 5. Read receipts and care history

A receipt is minted only when a voucher settles. It is bound to the same
`beneficiary_ref` and is not transferable.

```js
import { arg, contracts, read } from './sci.mjs';

// Throws ReceiptNotFound (#4) if the voucher did not settle.
const receipt = await read(contracts.receipt, 'get_receipt', [arg.u64(voucherId)]);

console.log({
  voucherId: String(receipt.voucher_id),
  provider: receipt.provider,
  serviceCode: receipt.service_code,
  amount: receipt.amount,
  beneficiaryRef: Buffer.from(receipt.beneficiary_ref).toString('hex'),
  settledAt: new Date(Number(receipt.settled_at) * 1000),
});

// How many settled episodes this reference has. The contract does not expose
// an enumeration, so use the indexer API for a full history.
const count = await read(contracts.receipt, 'count_for', [
  arg.bytes32(beneficiaryRef.toString('hex')),
]);
console.log('settled episodes', Number(count));
```

The contract stores receipts by voucher id and exposes `count_for(ref)`, not a
list. For a full history, query the read-only indexer:

```js
const refHex = beneficiaryRef.toString('hex');
const response = await fetch(
  `https://sci-healthcare-api.onrender.com/receipts?beneficiaryRef=${refHex}`,
);
if (!response.ok) throw new Error(`Receipt history failed: ${response.status}`);
const history = await response.json();
console.log(history.receipts);
```

The indexer holds only public on-chain data and needs the same 64-character
reference. It cannot enumerate patients without the key.

## Error handling

Check every simulated call before signing. The helper does this for you and maps
the common voucher errors:

| Code | Error | What to tell the user |
| --- | --- | --- |
| 3 | `NotAuthorized` | The connected wallet is not the funder/provider/attester named in the call. |
| 4 | `VoucherNotFound` / `ReceiptNotFound` | The id does not exist, or the voucher has not settled yet. |
| 5 | `ProviderNotActive` | The clinic is not verified or is suspended. |
| 6 | `ServiceNotOffered` | The clinic no longer lists that service code. |
| 7 | `AmountBelowPrice` | The amount is below the clinic's current listed price. |
| 8 | `InvalidAmount` | The amount must be greater than zero. |
| 9 | `InvalidExpiry` | The expiry must be in the future. |
| 10 | `InvalidStatus` | The voucher is not in a state where that action is allowed. |
| 11 | `VoucherExpired` | The voucher expired before the clinic claimed it. |
| 12 | `NotYetExpired` | The refund is not available yet; show `refundable_at`. |
| 13 | `DisputeWindowOpen` | Settlement is not available until the dispute deadline. |
| 14 | `DisputeWindowClosed` | The funder can no longer dispute. |
| 15 | `InvalidFee` | The deployment's fee exceeds the 10% cap; contact the operator. |
| 18 | `ServiceUnavailable` | The clinic has paused this service for now; try again later or choose another service. |

Recommended handling:

```js
try {
  const { hash } = await invoke(secret, contracts.voucher, 'create_voucher', args);
  console.log('submitted', hash);
} catch (error) {
  if (error.contractError) {
    // A contract rejection. Show the human-readable cause; do not retry blindly.
    console.error(error.contractError, error.code);
  } else {
    // An RPC or network failure. The transaction may or may not have been sent.
    console.error('Could not submit:', error.message);
  }
}
```

- A simulation error is safe: nothing was signed or sent.
- Once `sendTransaction` returns a hash, poll `getTransaction` until `SUCCESS`
  or `FAILED`. Do not resubmit only because confirmation is slow; check the
  explorer first.
- Treat `Transaction rejected` and `FAILED` as terminal for that build.
- Never surface raw XDR to users.

## Privacy checklist for integrators

- [ ] The patient identifier and HMAC key are entered and kept on the device.
- [ ] The identifier and key never reach your server, logs, analytics or the chain.
- [ ] Only the 32-byte `beneficiary_ref` is submitted.
- [ ] Service codes stay coarse categories, never diagnoses or free text.
- [ ] The key can be exported and restored by the user; losing it loses the history.
- [ ] Your UI says the reference is pseudonymous, not anonymous.
- [ ] Your logs do not contain the reference next to a patient name.

See [Privacy](../protocol/privacy.md) for the full model.
