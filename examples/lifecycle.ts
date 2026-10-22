#!/usr/bin/env tsx
/**
 * SCI Healthcare — the whole voucher lifecycle in TypeScript.
 *
 * The web app drives fund → claim → attest → settle through Freighter. This
 * script does the same journey with @stellar/stellar-sdk and local keys, so
 * a developer can watch the protocol run without a browser. It mirrors
 * scripts/seed.sh (the CLI version) but finishes the story: it waits out the
 * dispute window, calls `settle`, and prints the receipt the voucher
 * contract minted on settlement.
 *
 * Usage (from examples/):
 *
 *   npm install
 *   ADMIN_SECRET=<seed of your deployment's admin> npm run lifecycle
 *   npm run lifecycle -- settle 7     # finish a voucher whose window has opened
 *
 * Contract ids come from deployments/testnet.env, which scripts/deploy.sh
 * writes (env vars REGISTRY_CONTRACT_ID etc. override it; without either,
 * the published testnet ids from the README are used — but only the
 * maintainer holds that deployment's admin key, so a full run needs your
 * own deployment).
 *
 * Keys: ADMIN_SECRET is required and must belong to the deployment's admin
 * (the identity deploy.sh used). CLINIC_SECRET, ATTESTER_SECRET and
 * FUNDER_SECRET are optional — a missing one is generated and printed so
 * you can re-export it next time. ISSUER_SECRET is only needed when the
 * deployment has no faucet contract.
 *
 * Testnet only: the RPC endpoint, passphrase and friendbot are hard-coded.
 */

// Must run before the SDK import: see webcrypto.ts.
import './webcrypto.js';

import { createHash } from 'node:crypto';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  Account,
  Address,
  Asset,
  BASE_FEE,
  Contract,
  Horizon,
  Keypair,
  Networks,
  Operation,
  TransactionBuilder,
  nativeToScVal,
  rpc,
  scValToNative,
  xdr,
} from '@stellar/stellar-sdk';

// ----- configuration -----

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');

/** Published testnet ids (README table) — used when no deployment file exists. */
const PUBLISHED = {
  REGISTRY_CONTRACT_ID: 'CAMU635NRFEHATJ6MTY5SI43IAZ43KHPFXAQDQAST2AHKCHLSGWHVE4V',
  VOUCHER_CONTRACT_ID: 'CBPUQN6CNJCNGPKNEXB7UM4T7RINKQB7HQI5CXEOSQ5EFRHE4G4VKPSQ',
  RECEIPT_CONTRACT_ID: 'CBCJRI2BCZBMZNX77WWDC2NGSV4ZNAN5Q23XHYNBH6UTOQMPJHZGX5C7',
  USDC_CONTRACT_ID: 'CCRLJ7FEIKTR3GBT3VQQPSIJGRBO2WPNMVFKJDP6AYGJQWECOR4QVE5M',
  USDC_ISSUER: 'GCKX5KAFVF7GLFZ3JWQGJ4F62RYYZU67PI3TACBGKVPJ73JELMI3L4KR',
};

/** deployments/testnet.env written by scripts/deploy.sh, then real env, then defaults. */
function loadConfig(): Record<string, string> {
  const file = join(ROOT, 'deployments', 'testnet.env');
  const fromFile: Record<string, string> = {};
  if (existsSync(file)) {
    for (const line of readFileSync(file, 'utf8').split('\n')) {
      const match = /^\s*([A-Z0-9_]+)\s*=\s*(.*?)\s*$/.exec(line);
      if (match && match[1] !== undefined && match[2] !== undefined) {
        fromFile[match[1]] = match[2];
      }
    }
  }
  return { ...PUBLISHED, ...fromFile, ...process.env };
}

const cfg = loadConfig();

const REGISTRY_ID = cfg.REGISTRY_CONTRACT_ID as string;
const VOUCHER_ID = cfg.VOUCHER_CONTRACT_ID as string;
const TOKEN_ID = cfg.USDC_CONTRACT_ID as string;
const USDC_ISSUER = cfg.USDC_ISSUER as string;
const FAUCET_ID = cfg.FAUCET_CONTRACT_ID ?? null;

const NETWORK_PASSPHRASE = Networks.TESTNET;
const RPC_URL = 'https://soroban-testnet.stellar.org';
const HORIZON_URL = 'https://horizon-testnet.stellar.org';
const EXPLORER = 'https://stellar.expert/explorer/testnet/tx';

const rpcServer = new rpc.Server(RPC_URL, { allowHttp: true });
const horizon = new Horizon.Server(HORIZON_URL);

/** Demo service, as seeded by scripts/seed.sh. */
const SERVICE_CODE = 101;
const SERVICE_LABEL = 'Outpatient consult';
const SERVICE_PRICE = 30_000_000n; // 3.00 USDC, 7 decimals
const MINT_AMOUNT = 1_000_000_000n; // 100 USDC when there is no faucet
/** How long the script will wait for a dispute window to close (seconds). */
const SETTLE_WAIT_SECONDS = Number(process.env.SETTLE_WAIT_SECONDS ?? '900');

const STATUS_NAMES = ['Funded', 'Claimed', 'Attested', 'Settled', 'Disputed', 'Refunded'] as const;

interface VoucherRecord {
  id: bigint;
  funder: string;
  provider: string;
  service_code: number;
  amount: bigint;
  status: number;
  created_at: bigint;
  expires_at: bigint;
  claimed_at: bigint;
  attested_at: bigint;
  dispute_deadline: bigint;
}

interface VoucherConfig {
  admin: string;
  registry: string;
  receipt_book: string;
  token: string;
  dispute_window: bigint;
  fee_bps: number;
  fee_account: string;
}

interface ReceiptRecord {
  voucher_id: bigint;
  provider: string;
  service_code: number;
  amount: bigint;
  settled_at: bigint;
}

// ----- small helpers -----

const sleep = (ms: number): Promise<void> => new Promise((r) => setTimeout(r, ms));

const say = (msg: string): void => console.log(`\n==> ${msg}`);
const note = (msg: string): void => console.log(`    ${msg}`);

/** 7-decimal base units → "30.00 USDC" (display only). */
function usdc(units: bigint): string {
  return `${(Number(units) / 10_000_000).toFixed(2)} USDC`;
}

function statusName(v: VoucherRecord): string {
  return STATUS_NAMES[v.status] ?? `unknown(${v.status})`;
}

const addr = (a: string): xdr.ScVal => Address.fromString(a).toScVal();
const u32 = (v: number): xdr.ScVal => nativeToScVal(v, { type: 'u32' });
const u64 = (v: bigint | number): xdr.ScVal => nativeToScVal(BigInt(v), { type: 'u64' });
const i128 = (v: bigint): xdr.ScVal => nativeToScVal(v, { type: 'i128' });
const str = (v: string): xdr.ScVal => nativeToScVal(v, { type: 'string' });

/** BytesN<32> is raw bytes, not a hex string (see scripts/seed.sh). */
const bytes32 = (hex: string): xdr.ScVal => xdr.ScVal.scvBytes(Buffer.from(hex, 'hex'));

/** Recorded for the run summary and the PR. */
const hashes: Array<[string, string]> = [];
function record(step: string, hash: string): void {
  hashes.push([step, hash]);
  note(`${step.padEnd(10)} tx ${hash}`);
  note(`           ${EXPLORER}/${hash}`);
}

// ----- keys -----

function requiredKey(envName: string, hint: string): Keypair {
  const secret = process.env[envName];
  if (!secret) throw new Error(`Missing ${envName}. ${hint}`);
  return Keypair.fromSecret(secret);
}

/**
 * Uses the given secret, or generates one and prints it — the script
 * friendbots whatever it creates, so a first run needs only ADMIN_SECRET.
 */
function keyOrCreate(envName: string, role: string): Keypair {
  const secret = process.env[envName];
  if (secret) return Keypair.fromSecret(secret);
  const kp = Keypair.random();
  console.log(`    generated ${envName} for this run — export it to reuse the ${role}:`);
  console.log(`      export ${envName}=${kp.secret()}`);
  return kp;
}

// ----- RPC plumbing (contract calls) -----

const CONFIRM_TIMEOUT_MS = 60_000;
const POLL_INTERVAL_MS = 1_500;

async function waitForTransaction(
  hash: string,
  label: string,
): Promise<rpc.Api.GetSuccessfulTransactionResponse> {
  const deadline = Date.now() + CONFIRM_TIMEOUT_MS;
  while (Date.now() < deadline) {
    const res = await rpcServer.getTransaction(hash);
    if (res.status === rpc.Api.GetTransactionStatus.SUCCESS) return res;
    if (res.status === rpc.Api.GetTransactionStatus.FAILED) {
      throw new Error(`${label}: transaction failed on chain — ${hash}`);
    }
    await sleep(POLL_INTERVAL_MS);
  }
  throw new Error(`${label}: ${hash} not confirmed in ${CONFIRM_TIMEOUT_MS / 1000}s — check ${EXPLORER}/${hash}`);
}

async function signSendWait(
  built: ReturnType<TransactionBuilder['build']>,
  signer: Keypair,
  label: string,
): Promise<{ hash: string; returnValue: unknown }> {
  built.sign(signer);
  const sent = await rpcServer.sendTransaction(built);
  if (sent.status === 'ERROR') {
    throw new Error(`${label}: rejected — ${JSON.stringify(sent.errorResult ?? sent.status)}`);
  }
  const hash = 'hash' in sent ? sent.hash : undefined;
  if (!hash) throw new Error(`${label}: RPC returned ${sent.status} without a transaction hash`);
  const result = await waitForTransaction(hash, label);
  return { hash, returnValue: result.returnValue ? scValToNative(result.returnValue) : null };
}

/**
 * Simulates, assembles, signs and submits a contract call. Simulation runs
 * before anything is signed, so a doomed call surfaces its contract error
 * here instead of wasting a transaction.
 */
async function invoke(
  contractId: string,
  method: string,
  args: xdr.ScVal[],
  signer: Keypair,
  label: string,
): Promise<{ hash: string; returnValue: unknown }> {
  const account = await rpcServer.getAccount(signer.publicKey());
  const contract = new Contract(contractId);
  const built = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK_PASSPHRASE,
  })
    .addOperation(contract.call(method, ...args))
    .setTimeout(180)
    .build();

  const sim = await rpcServer.simulateTransaction(built);
  if (rpc.Api.isSimulationError(sim)) throw new Error(`${label}: ${sim.error}`);
  const prepared = rpc.assembleTransaction(built, sim).build();
  return signSendWait(prepared, signer, label);
}

/** Reads a view function. Costs nothing and needs no signature. */
async function read<T>(contractId: string, method: string, args: xdr.ScVal[] = []): Promise<T> {
  const contract = new Contract(contractId);
  // A well-formed but unfunded account is enough to simulate a read.
  const dummy = new Account(
    'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF',
    '0',
  );
  const tx = new TransactionBuilder(dummy, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK_PASSPHRASE,
  })
    .addOperation(contract.call(method, ...args))
    .setTimeout(30)
    .build();

  const sim = await rpcServer.simulateTransaction(tx);
  if (rpc.Api.isSimulationError(sim)) throw new Error(`${method}: ${sim.error}`);
  if (!sim.result?.retval) throw new Error(`${method} returned nothing`);
  return scValToNative(sim.result.retval) as T;
}

/** For probes where "the contract said no" is a normal answer, not a failure. */
async function tryRead<T>(contractId: string, method: string, args: xdr.ScVal[] = []): Promise<T | null> {
  try {
    return await read<T>(contractId, method, args);
  } catch {
    return null;
  }
}

// ----- accounts, trustlines, USDC -----

async function accountExists(address: string): Promise<boolean> {
  try {
    await rpcServer.getAccount(address);
    return true;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    if (/not found|404/i.test(message)) return false;
    throw err;
  }
}

async function ensureAccount(kp: Keypair, role: string): Promise<void> {
  if (await accountExists(kp.publicKey())) return;
  note(`funding ${role} with friendbot: ${kp.publicKey()}`);
  const res = await fetch(`https://friendbot.stellar.org/?addr=${kp.publicKey()}`);
  if (!res.ok) throw new Error(`friendbot failed for ${role}: ${res.status} ${await res.text()}`);
  for (let i = 0; i < 10; i++) {
    if (await accountExists(kp.publicKey())) return;
    await sleep(1000);
  }
  throw new Error(`${role} was not visible after friendbot funding`);
}

/** A classic account needs a trustline before it can hold the SAC token. */
async function ensureTrustline(kp: Keypair, asset: Asset, role: string): Promise<void> {
  const account = await horizon.loadAccount(kp.publicKey());
  const has = account.balances.some(
    (b) => 'asset_code' in b && b.asset_code === asset.code && b.asset_issuer === asset.issuer,
  );
  if (has) return;

  note(`adding USDC trustline for ${role}`);
  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK_PASSPHRASE,
  })
    .addOperation(Operation.changeTrust({ line: asset }))
    .setTimeout(60)
    .build();
  tx.sign(kp);
  const res = await horizon.submitTransaction(tx);
  if (!res.successful) throw new Error(`trustline for ${role} failed: ${res.hash}`);
  note(`           tx ${res.hash}`);
}

/** USDC balance via the token contract; null means no trustline yet. */
async function usdcBalance(token: string, who: string): Promise<bigint | null> {
  return tryRead<bigint>(token, 'balance', [addr(who)]);
}

async function ensureUsdc(
  token: string,
  funder: Keypair,
  needed: bigint,
  asset: Asset,
): Promise<void> {
  const have = await usdcBalance(token, funder.publicKey());
  if (have !== null && have >= needed) {
    note(`funder already holds ${usdc(have)}`);
    return;
  }
  await ensureTrustline(funder, asset, 'funder');

  if (FAUCET_ID) {
    try {
      const { hash } = await invoke(FAUCET_ID, 'drip', [addr(funder.publicKey())], funder, 'faucet drip');
      record('faucet', hash);
      return;
    } catch (err) {
      note(`faucet drip unavailable (${err instanceof Error ? err.message : err})`);
    }
  }

  const issuer = process.env.ISSUER_SECRET;
  if (!issuer) {
    throw new Error(
      `Funder needs ${usdc(needed)} but has none. Deploy the faucet ` +
        '(./scripts/deploy-faucet.sh) or set ISSUER_SECRET so the example can mint.',
    );
  }
  const { hash } = await invoke(
    token,
    'mint',
    [addr(funder.publicKey()), i128(MINT_AMOUNT)],
    Keypair.fromSecret(issuer),
    'mint test USDC',
  );
  record('mint', hash);
}

// ----- registry setup -----

async function ensureRegistryRoles(
  admin: Keypair,
  clinic: Keypair,
  attester: Keypair,
  registry: string,
): Promise<void> {
  const clinicAddr = clinic.publicKey();
  const attesterAddr = attester.publicKey();

  const existing = await tryRead(registry, 'get_provider', [addr(clinicAddr)]);
  if (existing === null) {
    const { hash } = await invoke(
      registry,
      'register_provider',
      [addr(clinicAddr), str('Example Clinic'), str('NG')],
      clinic,
      'register',
    );
    record('register', hash);
  }

  const active = await read<boolean>(registry, 'is_active_provider', [addr(clinicAddr)]);
  if (!active) {
    // ProviderStatus: 0 Pending, 1 Active, 2 Suspended (unit enum → u32).
    const { hash } = await invoke(
      registry,
      'set_provider_status',
      [addr(admin.publicKey()), addr(clinicAddr), u32(1)],
      admin,
      'activate provider',
    );
    record('activate', hash);
  }

  const price = await read<bigint>(registry, 'get_service_price', [addr(clinicAddr), u32(SERVICE_CODE)]);
  if (price === 0n) {
    const { hash } = await invoke(
      registry,
      'upsert_service',
      [addr(clinicAddr), u32(SERVICE_CODE), str(SERVICE_LABEL), i128(SERVICE_PRICE)],
      clinic,
      'list service',
    );
    record('service', hash);
  }

  const isAttester = await read<boolean>(registry, 'is_attester', [addr(attesterAddr)]);
  if (!isAttester) {
    const { hash } = await invoke(
      registry,
      'add_attester',
      [addr(admin.publicKey()), addr(attesterAddr)],
      admin,
      'add attester',
    );
    record('attester', hash);
  }
}

// ----- the dispute window and settlement -----

/**
 * Waits until `dispute_deadline`, then calls `settle` (permissionless, so
 * any local key may sign). Returns false when the window outlasts
 * SETTLE_WAIT_SECONDS — the caller then prints a `settle` command to run
 * later instead of failing.
 */
async function settleWhenReady(
  voucherId: bigint,
  signer: Keypair,
  voucherContract: string,
): Promise<boolean> {
  const voucher = await read<VoucherRecord>(voucherContract, 'get_voucher', [u64(voucherId)]);
  if (voucher.status === 3) {
    note(`voucher ${voucherId} is already Settled`);
    return true;
  }
  if (voucher.status !== 2) {
    throw new Error(`voucher ${voucherId} is ${statusName(voucher)} — only an Attested voucher settles`);
  }

  const deadlineSec = Number(voucher.dispute_deadline);
  const giveUp = Date.now() + SETTLE_WAIT_SECONDS * 1000;
  if (deadlineSec - Date.now() / 1000 > SETTLE_WAIT_SECONDS) return false;

  // Retry across the boundary. The deadline is ledger time and the clock
  // here is local, and testnet ledgers can lag it by tens of seconds, so a
  // `DisputeWindowOpen` rejection means "not yet" — poll until it clears.
  while (Date.now() < giveUp) {
    const nowSec = Math.floor(Date.now() / 1000);
    if (nowSec < deadlineSec - 15) {
      await sleep(Math.min(5000, Math.max(1000, (deadlineSec - nowSec) * 1000)));
      continue;
    }
    try {
      const { hash } = await invoke(voucherContract, 'settle', [u64(voucherId)], signer, 'settle');
      record('settle', hash);
      return true;
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      // Error #13 is VoucherError::DisputeWindowOpen (see contracts/voucher).
      const windowStillOpen = /DisputeWindowOpen|#13/.test(message);
      if (!windowStillOpen && nowSec >= deadlineSec + 15) throw err;
      if (Date.now() >= giveUp) throw err;
      await sleep(5000);
    }
  }
  return false;
}

function printSettleLater(voucherId: bigint): void {
  console.log(`
The dispute window is still open (deployments default to 72 hours).
This run funded, claimed and attested voucher ${voucherId}. When the window
closes, finish it from examples/ with:

  npm run lifecycle -- settle ${voucherId}`);
}

// ----- the run -----

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

async function settleOnly(): Promise<void> {
  const raw = process.argv[3];
  if (!raw || !/^\d+$/.test(raw)) {
    throw new Error('Usage: npm run lifecycle -- settle <voucher-id>');
  }
  const voucherId = BigInt(raw);
  const signer = keyOrCreate('FUNDER_SECRET', 'signer (settle is permissionless)');
  await ensureAccount(signer, 'signer');

  const voucher = await read<VoucherRecord>(VOUCHER_ID, 'get_voucher', [u64(voucherId)]);
  note(`voucher #${voucherId} is ${statusName(voucher)}`);
  const settled = await settleWhenReady(voucherId, signer, VOUCHER_ID);
  if (!settled) {
    printSettleLater(voucherId);
    return;
  }
  const config = await read<VoucherConfig>(VOUCHER_ID, 'get_config');
  const receipt = await read<ReceiptRecord>(config.receipt_book, 'get_receipt', [u64(voucherId)]);
  note(`receipt settled_at ${new Date(Number(receipt.settled_at) * 1000).toISOString()}`);
  summarize(hashes);
}

function summarize(records: Array<[string, string]>): void {
  say('Transactions this run');
  for (const [step, hash] of records) {
    console.log(`    ${step.padEnd(10)} ${EXPLORER}/${hash}`);
  }
  console.log('\nLifecycle complete.');
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
