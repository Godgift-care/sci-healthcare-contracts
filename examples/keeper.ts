#!/usr/bin/env tsx
/**
 * SCI Healthcare — settlement keeper bot.
 *
 * A voucher only moves forward when somebody calls the contract:
 *
 *   - `settle(voucher_id)` pays a clinic once the dispute window on an
 *     `Attested` voucher has closed. It is permissionless.
 *   - `refund(voucher_id)` returns the escrow to the funder once a `Funded`
 *     voucher has expired (or its clinic is no longer active), or once a
 *     `Claimed` voucher is seven days past expiry. It is permissionless too.
 *
 * Without a keeper, a clinic has to remember to press settle, and an expired
 * voucher's money sits in escrow until the funder notices. This bot does the
 * remembering: it asks the backend API which vouchers are due, checks the
 * chain itself, simulates each call, and only then submits it.
 *
 * Usage (from examples/):
 *
 *   npm install
 *   KEEPER_SECRET=<seed of a funded account> npm run keeper
 *   npm run keeper -- --dry-run          # print what it would do, send nothing
 *   npm run keeper -- --watch            # keep running, one pass every 30s
 *   npm run keeper -- --dry-run --watch
 *
 * The keeper account pays the transaction fee and needs a little XLM. It does
 * not need a USDC trustline: settle and refund pay the clinic and the funder,
 * never the caller.
 *
 * Contract ids come from deployments/testnet.env, which scripts/deploy.sh
 * writes; env vars REGISTRY_CONTRACT_ID etc. override it. The backend URL
 * defaults to the local API (http://localhost:8080); set KEEPER_API_URL to
 * point at another one.
 *
 * Safety: a call is only sent after the chain state says it is due *and* a
 * simulation of the exact transaction succeeds. A simulation error, a status
 * change, or a dispute window that is still open means the voucher is skipped,
 * never submitted.
 *
 * Testnet is the default. For a local quickstart node set:
 *
 *   KEEPER_RPC_URL=http://localhost:8000/soroban/rpc
 *   KEEPER_NETWORK_PASSPHRASE='Standalone Network ; February 2017'
 */

// Must run before the SDK import: see webcrypto.ts.
import './webcrypto.js';

import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

import {
  Account,
  BASE_FEE,
  Contract,
  Keypair,
  Networks,
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
  VOUCHER_CONTRACT_ID: 'CBPUQN6CNJCNGPKNEXB7UM4T7RINKQB7HQI5CXEOSQ5EFRHE4G4VKPSQ',
};

/**
 * deployments/testnet.env written by scripts/deploy.sh, then real env, then
 * defaults. The keeper only needs the voucher contract: it reads the registry
 * and token ids from the voucher's own `get_config`.
 */
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

const VOUCHER_ID = cfg.VOUCHER_CONTRACT_ID as string;
const API_URL = (process.env.KEEPER_API_URL ?? 'http://localhost:8080').replace(/\/+$/, '');
const RPC_URL = process.env.KEEPER_RPC_URL ?? 'https://soroban-testnet.stellar.org';
const NETWORK_PASSPHRASE = process.env.KEEPER_NETWORK_PASSPHRASE ?? Networks.TESTNET;
const EXPLORER =
  process.env.KEEPER_EXPLORER ?? 'https://stellar.expert/explorer/testnet/tx';
/** One pass every 30 seconds unless KEEPER_POLL_SECONDS says otherwise. */
const POLL_SECONDS = Number(process.env.KEEPER_POLL_SECONDS ?? '30');
/** The backend limits a page to 100. */
const PAGE_SIZE = 100;

const rpcServer = new rpc.Server(RPC_URL, { allowHttp: true });

// ----- voucher state -----

/** `VoucherStatus` discriminants in contracts/voucher/src/types.rs. */
export const VOUCHER_STATUS = {
  Funded: 0,
  Claimed: 1,
  Attested: 2,
  Settled: 3,
  Disputed: 4,
  Refunded: 5,
} as const;

const STATUS_NAMES = ['Funded', 'Claimed', 'Attested', 'Settled', 'Disputed', 'Refunded'] as const;

export type Action = 'settle' | 'refund';

/** The fields of `Voucher` the keeper reasons about. */
export interface VoucherSnapshot {
  status: number;
  expires_at: bigint;
  dispute_deadline: bigint;
}

/** A voucher as `GET /vouchers` returns it (docs/developers/api.md). */
export interface ApiVoucher {
  id: string;
  status: string;
  funder?: string;
  provider?: unknown;
  amount?: string;
  expiresAt?: string | null;
  disputeDeadline?: string | null;
  refundableAt?: string | null;
  isSettleable?: boolean;
  isRefundable?: boolean;
}

export interface ApiVoucherPage {
  total?: number;
  limit?: number;
  offset?: number;
  vouchers?: ApiVoucher[];
}

export type FetchLike = (url: string) => Promise<Response>;

export interface DueVoucher {
  voucher: ApiVoucher;
  action: Action;
}

export type ChainDecision =
  | { proceed: true; action: Action; reason: string }
  | { proceed: false; action: Action; reason: string };

// ----- small helpers -----

const sleep = (ms: number): Promise<void> => new Promise((resolve) => setTimeout(resolve, ms));

const say = (message: string): void => console.log(`\n==> ${message}`);
const note = (message: string): void => console.log(`    ${message}`);

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

function utc(seconds: number): string {
  return new Date(seconds * 1000).toISOString();
}

export function statusName(status: number): string {
  return STATUS_NAMES[status] ?? `unknown(${status})`;
}

/**
 * Parses every time shape the API or RPC uses: Unix seconds as a string,
 * Unix milliseconds, an ISO date. Returns seconds, or `null` when the value is
 * missing or not a date at all.
 */
export function parseUnixTime(value: string | null | undefined): number | null {
  if (value === null || value === undefined) return null;
  const text = value.trim();
  if (!text) return null;

  if (/^-?\d+$/.test(text)) {
    const number = Number(text);
    if (!Number.isFinite(number)) return null;
    return Math.abs(number) >= 1_000_000_000_000
      ? Math.floor(number / 1000)
      : Math.floor(number);
  }

  const milliseconds = Date.parse(text);
  if (Number.isNaN(milliseconds)) return null;
  return Math.floor(milliseconds / 1000);
}

/** The provider is an object in `/vouchers` and a string in some clients. */
export function providerAddress(voucher: ApiVoucher): string | null {
  const provider = voucher.provider;
  if (typeof provider === 'string') return provider;
  if (provider !== null && typeof provider === 'object' && 'address' in provider) {
    const address = (provider as { address?: unknown }).address;
    if (typeof address === 'string') return address;
  }
  return null;
}

/**
 * Whether the API's own derived fields say a voucher is due. The chain check
 * in `decideFromChain` is what actually authorises the call; this only keeps
 * the scan small.
 */
export function dueActionFromApi(voucher: ApiVoucher, nowSec: number): Action | null {
  const status = voucher.status.toLowerCase();

  if (status === 'attested') {
    const deadline = parseUnixTime(voucher.disputeDeadline);
    if (voucher.isSettleable === true || (deadline !== null && deadline <= nowSec)) {
      return 'settle';
    }
    return null;
  }

  if (status === 'funded' || status === 'claimed') {
    const refundableAt = parseUnixTime(voucher.refundableAt);
    if (voucher.isRefundable === true || (refundableAt !== null && refundableAt <= nowSec)) {
      return 'refund';
    }
    return null;
  }

  return null;
}

// ----- backend discovery -----

/**
 * Reads the open vouchers from the backend and keeps the ones its derived
 * fields mark as due. The API requires a filter, so this asks for each open
 * status in turn and pages until `total` is reached.
 */
export async function fetchDueVouchers(
  apiUrl: string,
  nowSec: number,
  fetchImpl: FetchLike = fetch,
): Promise<DueVoucher[]> {
  const base = apiUrl.replace(/\/+$/, '');
  const due = new Map<string, DueVoucher>();

  for (const status of ['Attested', 'Funded', 'Claimed']) {
    let offset = 0;
    let total = Number.POSITIVE_INFINITY;

    while (offset < total) {
      const url = `${base}/vouchers?status=${encodeURIComponent(status)}&limit=${PAGE_SIZE}&offset=${offset}`;
      const response = await fetchImpl(url);
      if (!response.ok) {
        throw new Error(
          `GET /vouchers?status=${status} failed: ${response.status} ${await response.text()}`,
        );
      }

      const page = (await response.json()) as ApiVoucherPage;
      if (!Array.isArray(page.vouchers)) {
        throw new Error(`GET /vouchers?status=${status} returned no vouchers array`);
      }

      total = typeof page.total === 'number' ? page.total : offset + page.vouchers.length;
      for (const voucher of page.vouchers) {
        const action = dueActionFromApi(voucher, nowSec);
        if (action !== null) due.set(voucher.id, { voucher, action });
      }

      if (page.vouchers.length === 0) break;
      offset += page.vouchers.length;
    }
  }

  return [...due.values()];
}

// ----- chain preflight -----

/**
 * Decides from the chain, not from the indexer, whether the proposed call is
 * allowed. An API that is behind can nominate a voucher that is still early;
 * this is the gate that keeps the keeper from wasting a transaction on it.
 */
export function decideFromChain(
  action: Action,
  voucher: VoucherSnapshot,
  refundableAt: number | null,
  nowSec: number,
): ChainDecision {
  if (action === 'settle') {
    if (voucher.status !== VOUCHER_STATUS.Attested) {
      return {
        proceed: false,
        action,
        reason: `on chain it is ${statusName(voucher.status)}, not Attested`,
      };
    }
    const deadline = Number(voucher.dispute_deadline);
    if (!Number.isSafeInteger(deadline)) {
      return { proceed: false, action, reason: 'dispute_deadline does not fit in a JavaScript number' };
    }
    if (nowSec < deadline) {
      return {
        proceed: false,
        action,
        reason: `dispute window is open until ${utc(deadline)}`,
      };
    }
    return {
      proceed: true,
      action,
      reason: `dispute window closed at ${utc(deadline)}`,
    };
  }

  if (voucher.status !== VOUCHER_STATUS.Funded && voucher.status !== VOUCHER_STATUS.Claimed) {
    return {
      proceed: false,
      action,
      reason: `on chain it is ${statusName(voucher.status)}, which cannot be refunded`,
    };
  }
  if (refundableAt === null) {
    return { proceed: false, action, reason: 'refundable_at returned no time' };
  }
  if (nowSec < refundableAt) {
    return {
      proceed: false,
      action,
      reason: `not refundable until ${utc(refundableAt)}`,
    };
  }
  return {
    proceed: true,
    action,
    reason: refundableAt <= nowSec ? `refundable since ${utc(refundableAt)}` : 'refundable now',
  };
}

// ----- RPC plumbing -----

const CONFIRM_TIMEOUT_MS = 60_000;
const POLL_INTERVAL_MS = 1_500;

const u64 = (value: bigint): xdr.ScVal => nativeToScVal(value, { type: 'u64' });

async function ledgerNow(): Promise<number> {
  const latest = await rpcServer.getLatestLedger();
  const seconds = parseUnixTime(latest.closeTime);
  if (seconds === null) {
    throw new Error(`RPC returned an unreadable latest-ledger close time: ${latest.closeTime}`);
  }
  return seconds;
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

  const simulation = await rpcServer.simulateTransaction(tx);
  if (rpc.Api.isSimulationError(simulation)) throw new Error(`${method}: ${simulation.error}`);
  if (!simulation.result?.retval) throw new Error(`${method} returned nothing`);
  return scValToNative(simulation.result.retval) as T;
}

type BuiltTransaction = ReturnType<TransactionBuilder['build']>;

export type PreflightResult =
  | { ok: true; transaction: BuiltTransaction }
  | { ok: false; reason: string };

/**
 * Builds and simulates the exact call. A simulation error, or a simulation
 * that needs ledger entries restored first, means the keeper must not send it.
 */
async function preflight(
  action: Action,
  voucherId: bigint,
  keeper: Keypair,
): Promise<PreflightResult> {
  try {
    const account = await rpcServer.getAccount(keeper.publicKey());
    const built = new TransactionBuilder(account, {
      fee: BASE_FEE,
      networkPassphrase: NETWORK_PASSPHRASE,
    })
      .addOperation(new Contract(VOUCHER_ID).call(action, u64(voucherId)))
      .setTimeout(180)
      .build();

    const simulation = await rpcServer.simulateTransaction(built);
    if (rpc.Api.isSimulationError(simulation)) {
      return { ok: false, reason: simulation.error };
    }
    if (rpc.Api.isSimulationRestore(simulation)) {
      return {
        ok: false,
        reason: 'simulation needs a ledger-entry restore; read the voucher and retry next pass',
      };
    }

    return { ok: true, transaction: rpc.assembleTransaction(built, simulation).build() };
  } catch (err) {
    return { ok: false, reason: errorMessage(err) };
  }
}

async function waitForTransaction(
  hash: string,
  label: string,
): Promise<rpc.Api.GetSuccessfulTransactionResponse> {
  const deadline = Date.now() + CONFIRM_TIMEOUT_MS;
  while (Date.now() < deadline) {
    const result = await rpcServer.getTransaction(hash);
    if (result.status === rpc.Api.GetTransactionStatus.SUCCESS) return result;
    if (result.status === rpc.Api.GetTransactionStatus.FAILED) {
      throw new Error(`${label}: transaction failed on chain — ${hash}`);
    }
    await sleep(POLL_INTERVAL_MS);
  }
  throw new Error(
    `${label}: ${hash} not confirmed in ${CONFIRM_TIMEOUT_MS / 1000}s — check ${EXPLORER}/${hash}`,
  );
}

async function signSendWait(
  built: BuiltTransaction,
  signer: Keypair,
  label: string,
): Promise<string> {
  built.sign(signer);
  const sent = await rpcServer.sendTransaction(built);
  if (sent.status === 'ERROR') {
    throw new Error(`${label}: rejected — ${JSON.stringify(sent.errorResult ?? sent.status)}`);
  }
  const hash = 'hash' in sent ? sent.hash : undefined;
  if (!hash) throw new Error(`${label}: RPC returned ${sent.status} without a transaction hash`);
  await waitForTransaction(hash, label);
  return hash;
}

// ----- the keeper pass -----

export interface PassSummary {
  due: number;
  submitted: number;
  planned: number;
  skipped: number;
  failed: number;
}

export interface PassOptions {
  dryRun: boolean;
  /** `null` only in dry-run mode: without a key the keeper cannot simulate. */
  keeper: Keypair | null;
}

/**
 * One scan-and-act pass. Errors on one voucher are logged and do not stop the
 * rest: a keeper that dies on the first unreadable voucher is not a keeper.
 */
export async function runPass(options: PassOptions): Promise<PassSummary> {
  const summary: PassSummary = { due: 0, submitted: 0, planned: 0, skipped: 0, failed: 0 };
  const nowSec = await ledgerNow();
  const due = await fetchDueVouchers(API_URL, nowSec);
  summary.due = due.length;

  say(`Keeper pass at ${utc(nowSec)}`);
  note(`voucher contract ${VOUCHER_ID}`);
  note(`API ${API_URL}`);
  note(`${due.length} due voucher(s) reported by the backend`);

  for (const { voucher, action } of due) {
    const idText = voucher.id;
    if (!/^\d+$/.test(idText)) {
      note(`voucher ${idText}: skip — not a numeric voucher id`);
      summary.skipped += 1;
      continue;
    }
    const id = BigInt(idText);

    try {
      const snapshot = await read<VoucherSnapshot>(VOUCHER_ID, 'get_voucher', [u64(id)]);
      let refundableAt: number | null = null;
      if (action === 'refund') {
        const raw = await read<bigint>(VOUCHER_ID, 'refundable_at', [u64(id)]);
        refundableAt = Number(raw);
      }

      const decision = decideFromChain(action, snapshot, refundableAt, nowSec);
      if (!decision.proceed) {
        note(`voucher ${id}: skip — ${decision.reason}`);
        summary.skipped += 1;
        continue;
      }

      if (!options.keeper) {
        note(`voucher ${id}: dry run — would ${action} (${decision.reason})`);
        summary.planned += 1;
        continue;
      }

      const result = await preflight(action, id, options.keeper);
      if (!result.ok) {
        note(`voucher ${id}: skip — preflight rejected the ${action}: ${result.reason}`);
        summary.skipped += 1;
        continue;
      }

      if (options.dryRun) {
        note(`voucher ${id}: dry run — simulation passed, would ${action} (${decision.reason})`);
        summary.planned += 1;
        continue;
      }

      const hash = await signSendWait(result.transaction, options.keeper, `${action} ${id}`);
      note(`voucher ${id}: ${action} submitted — ${EXPLORER}/${hash}`);
      summary.submitted += 1;
    } catch (err) {
      note(`voucher ${id}: failed — ${errorMessage(err)}`);
      summary.failed += 1;
    }
  }

  return summary;
}

// ----- command line -----

export interface KeeperArgs {
  dryRun: boolean;
  watch: boolean;
  help: boolean;
}

export function parseArgs(argv: string[]): KeeperArgs {
  const args: KeeperArgs = { dryRun: false, watch: false, help: false };

  for (const arg of argv) {
    if (arg === '--dry-run') args.dryRun = true;
    else if (arg === '--watch') args.watch = true;
    else if (arg === '--help' || arg === '-h') args.help = true;
    else throw new Error(`Unknown argument "${arg}". Use --dry-run, --watch or --help.`);
  }

  return args;
}

function printUsage(): void {
  console.log(`SCI Healthcare — settlement keeper

Usage: npm run keeper [-- --dry-run] [--watch]

  --dry-run   simulate and print actions; never sign or send
  --watch     keep running one pass every KEEPER_POLL_SECONDS (default 30)

Environment:
  KEEPER_SECRET                 funded Stellar secret seed (required to submit)
  KEEPER_API_URL                backend base URL (default http://localhost:8080)
  KEEPER_RPC_URL                Soroban RPC endpoint (default testnet)
  KEEPER_NETWORK_PASSPHRASE     network passphrase (default testnet)
  KEEPER_POLL_SECONDS           seconds between passes in --watch mode
  VOUCHER_CONTRACT_ID           overrides deployments/testnet.env`);
}

function keeperFromEnv(): Keypair | null {
  const secret = process.env.KEEPER_SECRET?.trim();
  if (!secret) return null;
  try {
    return Keypair.fromSecret(secret);
  } catch {
    throw new Error('KEEPER_SECRET is not a valid Stellar secret seed');
  }
}

async function ensureKeeperAccount(keeper: Keypair): Promise<void> {
  try {
    await rpcServer.getAccount(keeper.publicKey());
  } catch {
    throw new Error(
      `keeper account ${keeper.publicKey()} is not funded on this network. ` +
        'Fund it with testnet XLM (or the local friendbot) before submitting.',
    );
  }
}

function summarize(summary: PassSummary): void {
  note(
    `pass complete: ${summary.submitted} submitted, ${summary.planned} planned, ` +
      `${summary.skipped} skipped, ${summary.failed} failed`,
  );
}

async function main(): Promise<void> {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    printUsage();
    return;
  }

  const keeper = keeperFromEnv();
  if (!keeper && !args.dryRun) {
    throw new Error(
      'KEEPER_SECRET is required to submit transactions. Use --dry-run to print the plan ' +
        'without a key, or set KEEPER_SECRET to a funded account.',
    );
  }
  if (keeper && !args.dryRun) await ensureKeeperAccount(keeper);

  say('SCI Healthcare keeper');
  note(`network  ${NETWORK_PASSPHRASE}`);
  note(`rpc      ${RPC_URL}`);
  note(`api      ${API_URL}`);
  note(`mode     ${args.dryRun ? 'dry run' : 'submit'}${args.watch ? ', watching' : ', one pass'}`);

  if (!args.watch) {
    summarize(await runPass({ dryRun: args.dryRun, keeper }));
    return;
  }

  for (;;) {
    try {
      summarize(await runPass({ dryRun: args.dryRun, keeper }));
    } catch (err) {
      console.error(`\nkeeper pass failed: ${errorMessage(err)}`);
    }
    note(`waiting ${POLL_SECONDS}s before the next pass`);
    await sleep(POLL_SECONDS * 1000);
  }
}

const invokedDirectly =
  process.argv[1] !== undefined && pathToFileURL(process.argv[1]).href === import.meta.url;

if (invokedDirectly) {
  main().catch((err) => {
    console.error(`\n${errorMessage(err)}`);
    process.exit(1);
  });
}
