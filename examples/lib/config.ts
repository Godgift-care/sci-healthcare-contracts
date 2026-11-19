import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { Horizon, Networks, rpc } from '@stellar/stellar-sdk';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

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

export const REGISTRY_ID = cfg.REGISTRY_CONTRACT_ID as string;
export const VOUCHER_ID = cfg.VOUCHER_CONTRACT_ID as string;
export const TOKEN_ID = cfg.USDC_CONTRACT_ID as string;
export const USDC_ISSUER = cfg.USDC_ISSUER as string;
export const FAUCET_ID = cfg.FAUCET_CONTRACT_ID ?? null;

export const NETWORK_PASSPHRASE = Networks.TESTNET;
export const RPC_URL = 'https://soroban-testnet.stellar.org';
export const HORIZON_URL = 'https://horizon-testnet.stellar.org';
export const EXPLORER = 'https://stellar.expert/explorer/testnet/tx';

export const rpcServer = new rpc.Server(RPC_URL, { allowHttp: true });
export const horizon = new Horizon.Server(HORIZON_URL);

/** Demo service, as seeded by scripts/seed.sh. */
export const SERVICE_CODE = 101;
export const SERVICE_LABEL = 'Outpatient consult';
export const SERVICE_PRICE = 30_000_000n; // 3.00 USDC, 7 decimals
export const MINT_AMOUNT = 1_000_000_000n; // 100 USDC when there is no faucet
/** How long the script will wait for a dispute window to close (seconds). */
export const SETTLE_WAIT_SECONDS = Number(process.env.SETTLE_WAIT_SECONDS ?? '900');
