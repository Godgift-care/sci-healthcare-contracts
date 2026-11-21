import { Keypair } from '@stellar/stellar-sdk';

import { u64 } from './args.js';
import { SETTLE_WAIT_SECONDS } from './config.js';
import { note, record, sleep, statusName } from './output.js';
import { invoke, read } from './rpc.js';
import type { VoucherRecord } from './types.js';

export async function settleWhenReady(
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

  while (Date.now() < giveUp) {
    const nowSec = Math.floor(Date.now() / 1000);
    if (nowSec < deadlineSec - 15) {
      await sleep(Math.min(5000, Math.max(1000, (deadlineSec - nowSec) * 1000)));
      continue;
    }
    try {
      const { hash } = await invoke(
        voucherContract,
        'settle',
        [u64(voucherId)],
        signer,
        'settle',
      );
      record('settle', hash);
      return true;
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      const windowStillOpen = /DisputeWindowOpen|#13/.test(message);
      if (!windowStillOpen && nowSec >= deadlineSec + 15) throw err;
      if (Date.now() >= giveUp) throw err;
      await sleep(5000);
    }
  }
  return false;
}

export function printSettleLater(voucherId: bigint): void {
  console.log(`
The dispute window is still open (deployments default to 72 hours).
This run funded, claimed and attested voucher ${voucherId}. When the window
closes, finish it from examples/ with:

  npm run lifecycle -- settle ${voucherId}`);
}
