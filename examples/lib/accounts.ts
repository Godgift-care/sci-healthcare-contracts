import {
  Asset,
  BASE_FEE,
  Keypair,
  Operation,
  TransactionBuilder,
} from '@stellar/stellar-sdk';

import { addr, i128 } from './args.js';
import { FAUCET_ID, MINT_AMOUNT, NETWORK_PASSPHRASE, horizon, rpcServer } from './config.js';
import { note, record, sleep, usdc } from './output.js';
import { invoke, tryRead } from './rpc.js';

export async function accountExists(address: string): Promise<boolean> {
  try {
    await rpcServer.getAccount(address);
    return true;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
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
      const { hash } = await invoke(
        FAUCET_ID,
        'drip',
        [addr(funder.publicKey())],
        funder,
        'faucet drip',
      );
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
