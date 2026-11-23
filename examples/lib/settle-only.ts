import { u64 } from './args.js';
import { ensureAccount } from './accounts.js';
import { VOUCHER_ID } from './config.js';
import { keyOrCreate } from './keys.js';
import { hashes, note, statusName, summarize } from './output.js';
import { read } from './rpc.js';
import { printSettleLater, settleWhenReady } from './settlement.js';
import type { ReceiptRecord, VoucherConfig, VoucherRecord } from './types.js';

export async function settleOnly(): Promise<void> {
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
