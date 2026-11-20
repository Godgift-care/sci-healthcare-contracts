import './webcrypto.js';

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

export const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');

const PUBLISHED = {
  REGISTRY_CONTRACT_ID: 'CAMU635NRFEHATJ6MTY5SI43IAZ43KHPFXAQDQAST2AHKCHLSGWHVE4V',
  VOUCHER_CONTRACT_ID: 'CBPUQN6CNJCNGPKNEXB7UM4T7RINKQB7HQI5CXEOSQ5EFRHE4G4VKPSQ',
  RECEIPT_CONTRACT_ID: 'CBCJRI2BCZBMZNX77WWDC2NGSV4ZNAN5Q23XHYNBH6UTOQMPJHZGX5C7',
  USDC_CONTRACT_ID: 'CCRLJ7FEIKTR3GBT3VQQPSIJGRBO2WPNMVFKJDP6AYGJQWECOR4QVE5M',
  USDC_ISSUER: 'GCKX5KAFVF7GLFZ3JWQGJ4F62RYYZU67PI3TACBGKVPJ73JELMI3L4KR',
};

export function loadConfig(): Record<string, string> {
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

export const cfg = loadConfig();

export const REGISTRY_ID = cfg.REGISTRY_CONTRACT_ID as string;
export const VOUCHER_ID = cfg.VOUCHER_CONTRACT_ID as string;
export const TOKEN_ID = cfg.USDC_CONTRACT_ID as string;
export const USDC_ISSUER = cfg.USDC_ISSUER as string;
export const FAUCET_ID = cfg.FAUCET_CONTRACT_ID ?? null;
export const SPONSOR_ID = cfg.SPONSOR_CONTRACT_ID ?? null;

export const NETWORK_PASSPHRASE = Networks.TESTNET;
export const RPC_URL = 'https://soroban-testnet.stellar.org';
export const HORIZON_URL = 'https://horizon-testnet.stellar.org';
export const EXPLORER = 'https://stellar.expert/explorer/testnet/tx';

export const rpcServer = new rpc.Server(RPC_URL, { allowHttp: true });
export const horizon = new Horizon.Server(HORIZON_URL);

export const SERVICE_CODE = 101;
export const SERVICE_LABEL = 'Outpatient consult';
export const SERVICE_PRICE = 30_000_000n;
export const MINT_AMOUNT = 1_000_000_000n;

export const STATUS_NAMES = [
  'Funded',
  'Claimed',
  'Attested',
  'Settled',
  'Disputed',
  'Refunded',
] as const;

export interface VoucherRecord {
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

export interface VoucherConfig {
  admin: string;
  registry: string;
  receipt_book: string;
  token: string;
  dispute_window: bigint;
  fee_bps: number;
  fee_account: string;
}

export interface ReceiptRecord {
  voucher_id: bigint;
  provider: string;
  service_code: number;
  amount: bigint;
  settled_at: bigint;
}

export interface SponsorConfig {
  admin: string;
  registry: string;
  voucher: string;
  token: string;
  paused: boolean;
}

export interface SponsorRules {
  match_bps: number;
  cap_per_voucher: bigint;
  min_voucher_amount: bigint;
  min_voucher_age_secs: bigint;
  eligible_countries: string[];
  eligible_service_codes: number[];
}

export interface SponsorAccount {
  sponsor: string;
  deposited: bigint;
  committed: bigint;
  refunded: bigint;
  withdrawn: bigint;
  rules: SponsorRules;
  open_matches: Array<{ matched_voucher_id: bigint; amount: bigint }>;
}

export const sleep = (ms: number): Promise<void> => new Promise((r) => setTimeout(r, ms));

export const say = (msg: string): void => console.log(`\n==> ${msg}`);
export const note = (msg: string): void => console.log(`    ${msg}`);

export function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

export function usdc(units: bigint): string {
  const fixed = (Number(units) / 10_000_000).toFixed(7).replace(/0+$/, '').replace(/\.$/, '');
  const [whole = '0', fraction = ''] = fixed.split('.');
  return `${whole}.${fraction.padEnd(2, '0')} USDC`;
}

export function statusName(status: number): string {
  return STATUS_NAMES[status] ?? `unknown(${status})`;
}

export const addr = (a: string): xdr.ScVal => Address.fromString(a).toScVal();
export const u32 = (v: number): xdr.ScVal => nativeToScVal(v, { type: 'u32' });
export const u64 = (v: bigint | number): xdr.ScVal => nativeToScVal(BigInt(v), { type: 'u64' });
export const i128 = (v: bigint): xdr.ScVal => nativeToScVal(v, { type: 'i128' });
export const str = (v: string): xdr.ScVal => nativeToScVal(v, { type: 'string' });
export const bool = (v: boolean): xdr.ScVal => nativeToScVal(v, { type: 'bool' });
export const bytes32 = (hex: string): xdr.ScVal => xdr.ScVal.scvBytes(Buffer.from(hex, 'hex'));

export function requireConfig(name: string, hint: string): string {
  const value = cfg[name];
  if (!value) throw new Error(`Missing ${name}. ${hint}`);
  return value;
}

export function requiredKey(envName: string, hint: string): Keypair {
  const secret = process.env[envName];
  if (!secret) throw new Error(`Missing ${envName}. ${hint}`);
  return Keypair.fromSecret(secret);
}

export function keyOrCreate(envName: string, role: string): Keypair {
  const secret = process.env[envName];
  if (secret) return Keypair.fromSecret(secret);
  const kp = Keypair.random();
  console.log(`    generated ${envName} for this run — export it to reuse the ${role}:`);
  console.log(`      export ${envName}=${kp.secret()}`);
  return kp;
}

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
  throw new Error(
    `${label}: ${hash} not confirmed in ${CONFIRM_TIMEOUT_MS / 1000}s — check ${EXPLORER}/${hash}`,
  );
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

export async function invoke(
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

export async function read<T>(
  contractId: string,
  method: string,
  args: xdr.ScVal[] = [],
): Promise<T> {
  const contract = new Contract(contractId);
  const dummy = new Account('GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF', '0');
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

export async function tryRead<T>(
  contractId: string,
  method: string,
  args: xdr.ScVal[] = [],
): Promise<T | null> {
  try {
    return await read<T>(contractId, method, args);
  } catch {
    return null;
  }
}

export async function accountExists(address: string): Promise<boolean> {
  try {
    await rpcServer.getAccount(address);
    return true;
  } catch (err) {
    const message = errorMessage(err);
    if (/not found|404/i.test(message)) return false;
    throw err;
  }
}

export async function ensureAccount(kp: Keypair, role: string): Promise<void> {
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

export async function ensureTrustline(kp: Keypair, asset: Asset, role: string): Promise<void> {
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

export async function usdcBalance(token: string, who: string): Promise<bigint | null> {
  return tryRead<bigint>(token, 'balance', [addr(who)]);
}

export async function ensureUsdc(
  token: string,
  who: Keypair,
  needed: bigint,
  asset: Asset,
): Promise<void> {
  const have = await usdcBalance(token, who.publicKey());
  if (have !== null && have >= needed) {
    note(`${who.publicKey()} already holds ${usdc(have)}`);
    return;
  }
  await ensureTrustline(who, asset, who.publicKey());

  if (FAUCET_ID) {
    try {
      await invoke(FAUCET_ID, 'drip', [addr(who.publicKey())], who, 'faucet drip');
      return;
    } catch (err) {
      note(`faucet drip unavailable (${errorMessage(err)})`);
    }
  }

  const issuer = process.env.ISSUER_SECRET;
  if (!issuer) {
    throw new Error(
      `${who.publicKey()} needs ${usdc(needed)} but has none. Deploy the faucet ` +
        '(./scripts/deploy-faucet.sh) or set ISSUER_SECRET so the example can mint.',
    );
  }
  await invoke(
    token,
    'mint',
    [addr(who.publicKey()), i128(MINT_AMOUNT)],
    Keypair.fromSecret(issuer),
    'mint test USDC',
  );
}

export async function ensureRegistryRoles(
  admin: Keypair,
  clinic: Keypair,
  attester: Keypair,
  registry: string,
): Promise<void> {
  const clinicAddr = clinic.publicKey();
  const attesterAddr = attester.publicKey();

  const existing = await tryRead(registry, 'get_provider', [addr(clinicAddr)]);
  if (existing === null) {
    await invoke(
      registry,
      'register_provider',
      [addr(clinicAddr), str('Example Clinic'), str('NG')],
      clinic,
      'register provider',
    );
  }

  const active = await read<boolean>(registry, 'is_active_provider', [addr(clinicAddr)]);
  if (!active) {
    await invoke(
      registry,
      'set_provider_status',
      [addr(admin.publicKey()), addr(clinicAddr), u32(1), u32(1)],
      admin,
      'activate provider',
    );
  }

  const price = await read<bigint>(registry, 'get_service_price', [
    addr(clinicAddr),
    u32(SERVICE_CODE),
  ]);
  if (price === 0n) {
    await invoke(
      registry,
      'upsert_service',
      [addr(clinicAddr), u32(SERVICE_CODE), str(SERVICE_LABEL), i128(SERVICE_PRICE)],
      clinic,
      'list service',
    );
  }

  const isAttester = await read<boolean>(registry, 'is_attester', [addr(attesterAddr)]);
  if (!isAttester) {
    await invoke(
      registry,
      'add_attester',
      [addr(admin.publicKey()), addr(attesterAddr)],
      admin,
      'add attester',
    );
  }
}

export async function assertRegistryAdmin(admin: Keypair): Promise<void> {
  const registryAdmin = await read<string>(REGISTRY_ID, 'get_admin');
  if (registryAdmin !== admin.publicKey()) {
    throw new Error(
      `Registry admin is ${registryAdmin}, but ADMIN_SECRET is ${admin.publicKey()}.\n` +
        'This example must point at a deployment you control. Check ' +
        'deployments/testnet.env, or create one with ./scripts/deploy.sh testnet.',
    );
  }
}

export async function readVoucher(voucherId: bigint): Promise<VoucherRecord> {
  return read<VoucherRecord>(VOUCHER_ID, 'get_voucher', [u64(voucherId)]);
}

export async function fundVoucher(options: {
  funder: Keypair;
  provider: string;
  serviceCode?: number;
  amount?: bigint;
  beneficiaryRef: string;
  expiresAt?: bigint;
  label?: string;
}): Promise<{ voucherId: bigint; hash: string }> {
  const expiresAt =
    options.expiresAt ?? BigInt(Math.floor(Date.now() / 1000) + 30 * 24 * 60 * 60);
  const result = await invoke(
    VOUCHER_ID,
    'create_voucher',
    [
      addr(options.funder.publicKey()),
      bytes32(options.beneficiaryRef),
      addr(options.provider),
      u32(options.serviceCode ?? SERVICE_CODE),
      i128(options.amount ?? SERVICE_PRICE),
      u64(expiresAt),
    ],
    options.funder,
    options.label ?? 'fund voucher',
  );
  return { voucherId: result.returnValue as bigint, hash: result.hash };
}

export function summarize(records: Array<[string, string]>): void {
  if (records.length === 0) return;
  say('Transactions this run');
  for (const [step, hash] of records) {
    console.log(`    ${step.padEnd(12)} ${EXPLORER}/${hash}`);
  }
}
