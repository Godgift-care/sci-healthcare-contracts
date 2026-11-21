#!/usr/bin/env tsx

import { createHash } from 'node:crypto';
import { pathToFileURL } from 'node:url';

import { Asset, nativeToScVal, xdr } from '@stellar/stellar-sdk';

import {
  REGISTRY_ID,
  SERVICE_CODE,
  SERVICE_PRICE,
  SPONSOR_ID,
  TOKEN_ID,
  USDC_ISSUER,
  VOUCHER_ID,
  addr,
  assertRegistryAdmin,
  ensureAccount,
  ensureRegistryRoles,
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
  u64,
  usdc,
  usdcBalance,
  type SponsorAccount,
  type SponsorConfig,
  type SponsorRules,
  type VoucherConfig,
} from './common.js';

const BUDGET = 60_000_000n;
const MATCH_BPS = 10_000;

interface MatchRecord {
  original_voucher_id: bigint;
  matched_voucher_id: bigint;
  sponsor: string;
  amount: bigint;
  provider: string;
  service_code: number;
  matched_at: bigint;
}

export function rulesToScVal(rules: SponsorRules): xdr.ScVal {
  return nativeToScVal(rules, {
    type: {
      match_bps: ['symbol', 'u32'],
      cap_per_voucher: ['symbol', 'i128'],
      min_voucher_amount: ['symbol', 'i128'],
      min_voucher_age_secs: ['symbol', 'u64'],
      eligible_countries: ['symbol', 'string'],
      eligible_service_codes: ['symbol', 'u32'],
    },
  });
}

function requireSponsorId(): string {
  if (!SPONSOR_ID) {
    throw new Error(
      'Missing SPONSOR_CONTRACT_ID. Deploy the sponsor desk with ./scripts/deploy.sh ' +
        'or set SPONSOR_CONTRACT_ID to an existing deployment.',
    );
  }
  return SPONSOR_ID;
}

async function main(): Promise<void> {
  console.log('SCI Healthcare — sponsor desk example (Stellar testnet)');

  const sponsorContract = requireSponsorId();
  const admin = requiredKey(
    'ADMIN_SECRET',
    "It must be the seed of this deployment's admin (the identity scripts/deploy.sh used).",
  );
  const sponsor = keyOrCreate('SPONSOR_SECRET', 'sponsor');
  const funder = keyOrCreate('FUNDER_SECRET', 'funder');
  const clinic = keyOrCreate('CLINIC_SECRET', 'clinic');
  const attester = keyOrCreate('ATTESTER_SECRET', 'attester');
  const transactions: Array<[string, string]> = [];

  say('Configuration');
  note(`sponsor   ${sponsorContract}`);
  note(`voucher   ${VOUCHER_ID}`);
  note(`registry  ${REGISTRY_ID}`);
  note(`token     ${TOKEN_ID}`);

  const sponsorConfig = await read<SponsorConfig>(sponsorContract, 'get_config');
  const voucherConfig = await read<VoucherConfig>(VOUCHER_ID, 'get_config');
  if (sponsorConfig.registry !== REGISTRY_ID) {
    throw new Error(
      `Sponsor desk uses registry ${sponsorConfig.registry}, but REGISTRY_CONTRACT_ID is ${REGISTRY_ID}.`,
    );
  }
  if (sponsorConfig.voucher !== VOUCHER_ID) {
    throw new Error(
      `Sponsor desk uses voucher ${sponsorConfig.voucher}, but VOUCHER_CONTRACT_ID is ${VOUCHER_ID}.`,
    );
  }
  if (sponsorConfig.token !== TOKEN_ID || voucherConfig.token !== TOKEN_ID) {
    throw new Error('Sponsor desk, voucher escrow and USDC_CONTRACT_ID disagree about the token.');
  }
  if (sponsorConfig.admin !== admin.publicKey()) {
    throw new Error(
      `Sponsor admin is ${sponsorConfig.admin}, but ADMIN_SECRET is ${admin.publicKey()}.`,
    );
  }
  await assertRegistryAdmin(admin);

  const token = sponsorConfig.token;
  const asset = new Asset('USDC', USDC_ISSUER);

  say('Accounts');
  await ensureAccount(admin, 'admin');
  await ensureAccount(sponsor, 'sponsor');
  await ensureAccount(funder, 'funder');
  await ensureAccount(clinic, 'clinic');
  await ensureAccount(attester, 'attester');
  note(`admin     ${admin.publicKey()}`);
  note(`sponsor   ${sponsor.publicKey()}`);
  note(`funder    ${funder.publicKey()}`);
  note(`clinic    ${clinic.publicKey()}`);

  say('Trustlines and test USDC');
  await ensureUsdc(token, sponsor, BUDGET, asset);
  await ensureUsdc(token, funder, SERVICE_PRICE, asset);

  say('Registry: clinic, service, attester');
  await ensureRegistryRoles(admin, clinic, attester, sponsorConfig.registry);

  const rules: SponsorRules = {
    match_bps: MATCH_BPS,
    cap_per_voucher: SERVICE_PRICE,
    min_voucher_amount: 0n,
    min_voucher_age_secs: 0n,
    eligible_countries: ['NG'],
    eligible_service_codes: [SERVICE_CODE],
  };

  const sponsorBefore = await usdcBalance(token, sponsor.publicKey());
  const funderBefore = await usdcBalance(token, funder.publicKey());

  say('1/4 sponsor deposits a budget');
  const deposited = await read<SponsorAccount>(sponsorContract, 'get_sponsor', [
    addr(sponsor.publicKey()),
  ]);
  note(
    `existing available budget ${usdc(
      await read<bigint>(sponsorContract, 'available_budget', [addr(sponsor.publicKey())]),
    )}`,
  );
  const deposit = await invoke(
    sponsorContract,
    'deposit',
    [addr(sponsor.publicKey()), i128(BUDGET)],
    sponsor,
    'deposit',
  );
  transactions.push(['deposit', deposit.hash]);

  say('2/4 sponsor sets matching rules');
  const setRules = await invoke(
    sponsorContract,
    'set_rules',
    [addr(sponsor.publicKey()), rulesToScVal(rules)],
    sponsor,
    'set rules',
  );
  transactions.push(['set_rules', setRules.hash]);
  const storedRules = await read<SponsorRules | null>(sponsorContract, 'get_rules', [
    addr(sponsor.publicKey()),
  ]);
  if (storedRules === null) throw new Error('sponsor rules were not stored');
  note(
    `match ${storedRules.match_bps / 100}% capped at ${usdc(storedRules.cap_per_voucher)} ` +
      `for ${storedRules.eligible_countries.join(', ')} service ${storedRules.eligible_service_codes.join(', ')}`,
  );

  say('3/4 funder creates the original voucher');
  const beneficiaryRef = createHash('sha256').update('patient-sponsor-001').digest('hex');
  const funded = await fundVoucher({
    funder,
    provider: clinic.publicKey(),
    beneficiaryRef,
    label: 'fund original',
  });
  transactions.push(['fund', funded.hash]);
  const original = await readVoucher(funded.voucherId);
  note(`voucher #${original.id} ${statusName(original.status)} for ${usdc(original.amount)}`);

  say('4/4 sponsor matches the voucher');
  const matched = await invoke(
    sponsorContract,
    'match_voucher',
    [addr(sponsor.publicKey()), u64(funded.voucherId)],
    sponsor,
    'match voucher',
  );
  transactions.push(['match', matched.hash]);
  const matchedId = matched.returnValue as bigint;
  const matchingVoucher = await readVoucher(matchedId);
  const match = await read<MatchRecord>(sponsorContract, 'get_match', [u64(funded.voucherId)]);
  const accountAfter = await read<SponsorAccount>(sponsorContract, 'get_sponsor', [
    addr(sponsor.publicKey()),
  ]);
  const availableAfter = await read<bigint>(sponsorContract, 'available_budget', [
    addr(sponsor.publicKey()),
  ]);
  const [available, tokenBalance] = await read<[bigint, bigint]>(
    sponsorContract,
    'budget_summary',
  );
  const sponsorAfter = await usdcBalance(token, sponsor.publicKey());
  const funderAfter = await usdcBalance(token, funder.publicKey());

  say('Result');
  note(`original voucher #${original.id}: ${statusName((await readVoucher(original.id)).status)}`);
  note(`matching voucher #${matchingVoucher.id}: ${statusName(matchingVoucher.status)}`);
  note(`matched amount ${usdc(match.amount)} for service ${match.service_code}`);
  note(`sponsor committed ${usdc(accountAfter.committed)} in ${accountAfter.open_matches.length} open match(es)`);
  note(`available budget ${usdc(availableAfter)}; budget summary ${usdc(available)} / ${usdc(tokenBalance)}`);
  if (sponsorBefore !== null && sponsorAfter !== null) {
    note(`sponsor token balance ${usdc(sponsorBefore)} -> ${usdc(sponsorAfter)}`);
  }
  if (funderBefore !== null && funderAfter !== null) {
    note(`funder token balance ${usdc(funderBefore)} -> ${usdc(funderAfter)}`);
  }
  if (deposited.deposited > 0n) {
    note(`sponsor had deposited ${usdc(deposited.deposited)} before this run`);
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
