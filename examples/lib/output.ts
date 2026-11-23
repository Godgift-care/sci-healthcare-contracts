import { EXPLORER } from './config.js';
import type { VoucherRecord } from './types.js';
import { STATUS_NAMES } from './types.js';

export const sleep = (ms: number): Promise<void> => new Promise((r) => setTimeout(r, ms));

export const say = (msg: string): void => console.log(`\n==> ${msg}`);
export const note = (msg: string): void => console.log(`    ${msg}`);

/** 7-decimal base units → "30.00 USDC" (display only). */
export function usdc(units: bigint): string {
  return `${(Number(units) / 10_000_000).toFixed(2)} USDC`;
}

export function statusName(v: VoucherRecord): string {
  return STATUS_NAMES[v.status] ?? `unknown(${v.status})`;
}

/** Recorded for the run summary and the PR. */
export const hashes: Array<[string, string]> = [];

export function record(step: string, hash: string): void {
  hashes.push([step, hash]);
  note(`${step.padEnd(10)} tx ${hash}`);
  note(`           ${EXPLORER}/${hash}`);
}

export function summarize(records: Array<[string, string]>): void {
  say('Transactions this run');
  for (const [step, hash] of records) {
    console.log(`    ${step.padEnd(10)} ${EXPLORER}/${hash}`);
  }
  console.log('\nLifecycle complete.');
}
