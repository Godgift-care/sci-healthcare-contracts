import {
  Account,
  BASE_FEE,
  Contract,
  Keypair,
  TransactionBuilder,
  rpc,
  scValToNative,
  xdr,
} from '@stellar/stellar-sdk';

import { EXPLORER, NETWORK_PASSPHRASE, rpcServer } from './config.js';
import { sleep } from './output.js';

const CONFIRM_TIMEOUT_MS = 60_000;
const POLL_INTERVAL_MS = 1_500;

export async function waitForTransaction(
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

export async function signSendWait(
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
