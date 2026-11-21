#!/usr/bin/env tsx

import { createHash } from 'node:crypto';
import { pathToFileURL } from 'node:url';

import { Asset } from '@stellar/stellar-sdk';

import {
  REGISTRY_ID,
  SERVICE_CODE,
  SERVICE_PRICE,
  TOKEN_ID,
  USDC_ISSUER,
  VOUCHER_ID,
  addr,
  assertRegistryAdmin,
  ensureAccount,
  ensureRegistryRoles,
  ensureTrustline,
  ensureUsdc,
  errorMessage,
  fundVoucher,
  i128,
  invoke,
  keyOrCreate,
  note,
  read,
  readVoucher,
  requiredKey,
  say,
  statusName,
  summarize,
  u32,
  u64,
  usdc,
  usdcBalance,
  type ReceiptRecord,
  type VoucherConfig,
} from './common.js';

const DISPUTE_REASON = 2;
const SPLIT_TO_PROVIDER = SERVICE_PRICE / 2n;

async function balanceOrZero(token: string, who: string): Promise<bigint> {
  return (await usdcBalance(token, who)) ?? 0n;
}

async function main(): Promise<void> {
  console.log('SCI Healthcare — dispute and split resolution example (Stellar testnet)');

  const admin = requiredKey(
    'ADMIN_SECRET',
    "It must be the seed of this deployment's admin (the identity scripts/deploy.sh used).",
  );
  const funder = keyOrCreate('FUNDER_SECRET', 'funder');
  const clinic = keyOrCreate('CLINIC_SECRET', 'clinic');
  const attester = keyOrCreate('ATTESTER_SECRET', 'attester');
  const transactions: Array<[string, string]> = [];

  say('Configuration');
  note(`voucher   ${VOUCHER_ID}`);
  note(`registry  ${REGISTRY_ID}`);
  note(`token     ${TOKEN_ID}`);

  await assertRegistryAdmin(admin);
  const config = await read<VoucherConfig>(VOUCHER_ID, 'get_config');
  const token = config.token;
  const asset = new Asset('USDC', USDC_ISSUER);

  say('Accounts');
  await ensureAccount(admin, 'admin');
  await ensureAccount(funder, 'funder');
  await ensureAccount(clinic, 'clinic');
  await ensureAccount(attester, 'attester');
  note(`admin     ${admin.publicKey()}`);
  note(`funder    ${funder.publicKey()}`);
  note(`clinic    ${clinic.publicKey()}`);
  note(`attester  ${attester.publicKey()}`);
  note(`fee acct  ${config.fee_account}`);

  say('Trustlines and test USDC');
  await ensureTrustline(admin, asset, 'admin');
  await ensureTrustline(clinic, asset, 'clinic');
  await ensureTrustline(funder, asset, 'funder');
  if (config.fee_account !== admin.publicKey()) {
    const feeAccountBalance = await usdcBalance(token, config.fee_account);
    if (feeAccountBalance === null) {
      throw new Error(
        `fee account ${config.fee_account} has no USDC trustline; resolve_split cannot pay it.`,
      );
    }
  }
  await ensureUsdc(token, funder, SERVICE_PRICE, asset);

  say('Registry: clinic, service, attester');
  await ensureRegistryRoles(admin, clinic, attester, config.registry);

  const [fee, net] = await read<[bigint, bigint]>(VOUCHER_ID, 'quote', [i128(SERVICE_PRICE)]);
  note(`amount ${usdc(SERVICE_PRICE)} — fee ${usdc(fee)}, clinic receives ${usdc(net)}`);

  const funderBefore = await balanceOrZero(token, funder.publicKey());
  const clinicBefore = await balanceOrZero(token, clinic.publicKey());
  const feeBefore = await balanceOrZero(token, config.fee_account);
  const attesterBefore = await balanceOrZero(token, attester.publicKey());
  const escrowBefore = await balanceOrZero(token, VOUCHER_ID);

  say('1/6 funder escrows the voucher');
  const beneficiaryRef = createHash('sha256').update('patient-dispute-001').digest('hex');
  const funded = await fundVoucher({
    funder,
    provider: clinic.publicKey(),
    beneficiaryRef,
    label: 'fund',
  });
  transactions.push(['fund', funded.hash]);
  note(`voucher #${funded.voucherId}`);

  say('2/6 clinic claims');
  const claimed = await invoke(
    VOUCHER_ID,
    'claim',
    [addr(clinic.publicKey()), u64(funded.voucherId)],
    clinic,
    'claim',
  );
  transactions.push(['claim', claimed.hash]);

  say('3/6 attester confirms delivery');
  const attested = await invoke(
    VOUCHER_ID,
    'attest',
    [addr(attester.publicKey()), u64(funded.voucherId)],
    attester,
    'attest',
  );
  transactions.push(['attest', attested.hash]);

  say('4/6 funder disputes the claim');
  const disputed = await invoke(
    VOUCHER_ID,
    'dispute',
    [addr(funder.publicKey()), u64(funded.voucherId), u32(DISPUTE_REASON)],
    funder,
    'dispute',
  );
  transactions.push(['dispute', disputed.hash]);
  note(`voucher #${funded.voucherId} is ${statusName((await readVoucher(funded.voucherId)).status)}`);

  say('5/6 admin resolves the split');
  const resolved = await invoke(
    VOUCHER_ID,
    'resolve_split',
    [addr(admin.publicKey()), u64(funded.voucherId), i128(SPLIT_TO_PROVIDER)],
    admin,
    'resolve split',
  );
  transactions.push(['resolve', resolved.hash]);

  const final = await readVoucher(funded.voucherId);
  const receipt = await read<ReceiptRecord>(config.receipt_book, 'get_receipt', [
    u64(funded.voucherId),
  ]);

  const funderAfter = await balanceOrZero(token, funder.publicKey());
  const clinicAfter = await balanceOrZero(token, clinic.publicKey());
  const feeAfter = await balanceOrZero(token, config.fee_account);
  const attesterAfter = await balanceOrZero(token, attester.publicKey());
  const escrowAfter = await balanceOrZero(token, VOUCHER_ID);

  const funderDelta = funderAfter - funderBefore;
  const clinicDelta = clinicAfter - clinicBefore;
  const feeDelta = feeAfter - feeBefore;
  const attesterDelta = attesterAfter - attesterBefore;
  const escrowDelta = escrowAfter - escrowBefore;
  const total = funderDelta + clinicDelta + feeDelta + attesterDelta + escrowDelta;

  say('6/6 result');
  note(`voucher #${final.id} is ${statusName(final.status)}`);
  note(`receipt records ${usdc(receipt.amount)} paid to the clinic`);
  note(`funder ${usdc(funderBefore)} -> ${usdc(funderAfter)} (${usdc(funderDelta)})`);
  note(`clinic ${usdc(clinicBefore)} -> ${usdc(clinicAfter)} (${usdc(clinicDelta)})`);
  note(`fee acct ${usdc(feeBefore)} -> ${usdc(feeAfter)} (${usdc(feeDelta)})`);
  note(`attester ${usdc(attesterBefore)} -> ${usdc(attesterAfter)} (${usdc(attesterDelta)})`);
  note(`voucher escrow ${usdc(escrowBefore)} -> ${usdc(escrowAfter)} (${usdc(escrowDelta)})`);
  note(`sum of deltas ${usdc(total)}`);

  if (total !== 0n) {
    throw new Error(`value was not conserved: deltas sum to ${usdc(total)}`);
  }
  if (final.status !== 3) {
    throw new Error(`expected voucher #${final.id} to be Settled, got ${statusName(final.status)}`);
  }
  if (attesterDelta !== 0n) {
    throw new Error('resolve_split paid the attester; the split fee belongs to the fee account');
  }

  summarize(transactions);
}

const invokedDirectly =
  process.argv[1] !== undefined && pathToFileURL(process.argv[1]).href === import.meta.url;

if (invokedDirectly) {
  main().catch((err) => {
    console.error(`\n${errorMessage(err)}`);
    process.exit(1);
  });
}
