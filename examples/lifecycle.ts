#!/usr/bin/env tsx
/**
 * SCI Healthcare — the whole voucher lifecycle in TypeScript.
 *
 * Mirrors scripts/seed.sh but finishes: it waits out the dispute window,
 * calls `settle`, and prints the receipt the voucher contract minted.
 *
 * From examples/: npm install, then
 *   ADMIN_SECRET=<deployment admin seed> npm run lifecycle
 *   npm run lifecycle -- settle 7
 *
 * Ids come from deployments/testnet.env (env overrides, then published ids;
 * a full run needs your own deployment). ADMIN_SECRET is required;
 * CLINIC_SECRET, ATTESTER_SECRET and FUNDER_SECRET are generated and printed
 * when absent. ISSUER_SECRET is only needed without a faucet. Testnet only.
 */

// Must run before the SDK import: see webcrypto.ts.
import './webcrypto.js';

import { createHash } from 'node:crypto';

import { Asset } from '@stellar/stellar-sdk';

import { addr, bytes32, i128, u32, u64 } from './lib/args.js';
import { ensureAccount, ensureTrustline, ensureUsdc, usdcBalance } from './lib/accounts.js';
import {
  REGISTRY_ID,
  RPC_URL,
  SERVICE_CODE,
  SERVICE_PRICE,
  TOKEN_ID,
  USDC_ISSUER,
  VOUCHER_ID,
} from './lib/config.js';
import { keyOrCreate, requiredKey } from './lib/keys.js';
import { hashes, note, record, say, statusName, summarize, usdc } from './lib/output.js';
import { ensureRegistryRoles } from './lib/registry.js';
import { invoke, read } from './lib/rpc.js';
import { printSettleLater, settleWhenReady } from './lib/settlement.js';
import { settleOnly } from './lib/settle-only.js';
import type { ReceiptRecord, VoucherConfig, VoucherRecord } from './lib/types.js';

async function fullLifecycle(): Promise<void> {
  const admin = requiredKey(
    'ADMIN_SECRET',
    "It must be the seed of this deployment's admin (the identity scripts/deploy.sh used). " +
      'The example runs the admin-only registry calls: activate the clinic, authorise the attester.',
  );
  const clinic = keyOrCreate('CLINIC_SECRET', 'clinic');
  const attester = keyOrCreate('ATTESTER_SECRET', 'attester');
  const funder = keyOrCreate('FUNDER_SECRET', 'funder');

  say('Configuration');
  note(`voucher   ${VOUCHER_ID}`);
  note(`registry  ${REGISTRY_ID}`);
  note(`token     ${TOKEN_ID}`);
  note(`rpc       ${RPC_URL}`);

  // Fail early with a useful message instead of a NotAuthorized deep in the run.
  const registryAdmin = await read<string>(REGISTRY_ID, 'get_admin');
  if (registryAdmin !== admin.publicKey()) {
    throw new Error(
      `Registry admin is ${registryAdmin}, but ADMIN_SECRET is ${admin.publicKey()}.\n` +
        'This example must point at a deployment you control. Check ' +
        'deployments/testnet.env, or create one with ./scripts/deploy.sh testnet.',
    );
  }
  // The voucher's own config is the source of truth for its wiring.
  const config = await read<VoucherConfig>(VOUCHER_ID, 'get_config');
  const registry = config.registry;
  const token = config.token;
  const receiptBook = config.receipt_book;
  const asset = new Asset('USDC', USDC_ISSUER);

  say('Accounts');
  await ensureAccount(admin, 'admin');
  await ensureAccount(clinic, 'clinic');
  await ensureAccount(attester, 'attester');
  await ensureAccount(funder, 'funder');
  note(`admin     ${admin.publicKey()}`);
  note(`clinic    ${clinic.publicKey()}`);
  note(`attester  ${attester.publicKey()}`);
  note(`funder    ${funder.publicKey()}`);

  say('Trustlines and test USDC');
  await ensureTrustline(admin, asset, 'admin (fee account)');
  await ensureTrustline(clinic, asset, 'clinic');
  if (config.fee_account !== admin.publicKey()) {
    const feeAccountHasIt = (await usdcBalance(token, config.fee_account)) !== null;
    if (!feeAccountHasIt) {
      console.warn(
        `    warning: fee account ${config.fee_account} has no USDC trustline; settle may fail`,
      );
    }
  }
  await ensureUsdc(token, funder, SERVICE_PRICE, asset);
  const funderBefore = await usdcBalance(token, funder.publicKey());
  const clinicBefore = await usdcBalance(token, clinic.publicKey());

  say('Registry: clinic, service, attester');
  await ensureRegistryRoles(admin, clinic, attester, registry);

  say('Quote');
  const [fee, net] = await read<[bigint, bigint]>(VOUCHER_ID, 'quote', [i128(SERVICE_PRICE)]);
  note(`amount ${usdc(SERVICE_PRICE)} — fee ${usdc(fee)}, clinic receives ${usdc(net)}`);

  // Stands in for the client-side HMAC of a patient id; no PHI on chain.
  const beneficiaryRef = createHash('sha256').update('patient-demo-001').digest('hex');
  const expiresAt = BigInt(Math.floor(Date.now() / 1000) + 30 * 24 * 60 * 60);

  say('1/4 funder escrows the voucher');
  const funded = await invoke(
    VOUCHER_ID,
    'create_voucher',
    [
      addr(funder.publicKey()),
      bytes32(beneficiaryRef),
      addr(clinic.publicKey()),
      u32(SERVICE_CODE),
      i128(SERVICE_PRICE),
      u64(expiresAt),
    ],
    funder,
    'fund',
  );
  const voucherId = funded.returnValue as bigint;
  record('fund', funded.hash);
  note(`voucher #${voucherId}`);

  say('2/4 clinic claims');
  const claimed = await invoke(
    VOUCHER_ID,
    'claim',
    [addr(clinic.publicKey()), u64(voucherId)],
    clinic,
    'claim',
  );
  record('claim', claimed.hash);

  say('3/4 attester confirms delivery');
  const attested = await invoke(
    VOUCHER_ID,
    'attest',
    [addr(attester.publicKey()), u64(voucherId)],
    attester,
    'attest',
  );
  record('attest', attested.hash);
  const afterAttest = await read<VoucherRecord>(VOUCHER_ID, 'get_voucher', [u64(voucherId)]);
  const opensAt = new Date(Number(afterAttest.dispute_deadline) * 1000).toISOString();
  note(`status ${statusName(afterAttest)} — dispute window closes ${opensAt}`);

  say('4/4 dispute window, then settle');
  const settled = await settleWhenReady(voucherId, funder, VOUCHER_ID);
  if (!settled) {
    printSettleLater(voucherId);
    summarize(hashes);
    return;
  }

  const final = await read<VoucherRecord>(VOUCHER_ID, 'get_voucher', [u64(voucherId)]);
  const receipt = await read<ReceiptRecord>(receiptBook, 'get_receipt', [u64(voucherId)]);
  const episodes = await read<number>(receiptBook, 'count_for', [bytes32(beneficiaryRef)]);
  const funderAfter = await usdcBalance(token, funder.publicKey());
  const clinicAfter = await usdcBalance(token, clinic.publicKey());

  say('Receipt');
  note(`voucher #${final.id} is ${statusName(final)}`);
  note(`receipt settled_at ${new Date(Number(receipt.settled_at) * 1000).toISOString()}`);
  note(`beneficiary episodes on record: ${episodes}`);
  if (funderBefore !== null && funderAfter !== null) {
    note(`funder paid ${usdc(funderBefore - funderAfter)}`);
  }
  if (clinicBefore !== null && clinicAfter !== null) {
    note(`clinic received ${usdc(clinicAfter - clinicBefore)}`);
  }
  summarize(hashes);
}

async function main(): Promise<void> {
  console.log('SCI Healthcare — voucher lifecycle example (Stellar testnet)');
  const mode = process.argv[2];
  if (mode === 'settle') {
    await settleOnly();
  } else if (mode === undefined) {
    await fullLifecycle();
  } else {
    throw new Error(`Unknown command "${mode}". Use no arguments, or: settle <voucher-id>`);
  }
}

main().catch((err) => {
  console.error(`\n${err instanceof Error ? err.message : err}`);
  process.exit(1);
});
