#!/usr/bin/env tsx

import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

import { rpc, scValToNative, xdr } from '@stellar/stellar-sdk';

import { ROOT, RPC_URL, VOUCHER_ID, errorMessage, note, rpcServer, say } from './common.js';

const DEFAULT_CURSOR_FILE = join(ROOT, 'examples', '.events-cursor.json');
const DEFAULT_LIMIT = 100;
const START_BACKLOG = 1000;

export interface EventsArgs {
  contractId: string;
  cursorFile: string;
  reset: boolean;
  startLedger: number | null;
  limit: number;
}

export interface CursorState {
  contractId: string;
  cursor: string;
  updatedAt: string;
}

export interface DecodedEvent {
  name: string;
  topics: unknown[];
  value: unknown;
}

export interface RawEventLike {
  topic: xdr.ScVal[];
  value: xdr.ScVal;
}

export function parseEventsArgs(
  argv: string[],
  defaults: { contractId?: string; cursorFile?: string } = {},
): EventsArgs {
  const args: EventsArgs = {
    contractId: defaults.contractId ?? VOUCHER_ID,
    cursorFile: defaults.cursorFile ?? DEFAULT_CURSOR_FILE,
    reset: false,
    startLedger: null,
    limit: DEFAULT_LIMIT,
  };

  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--reset') {
      args.reset = true;
    } else if (arg === '--start') {
      const value = argv[++i];
      if (value === undefined) throw new Error('--start needs a ledger number');
      args.startLedger = parseLedger(value, '--start');
    } else if (arg?.startsWith('--start=')) {
      args.startLedger = parseLedger(arg.slice('--start='.length), '--start');
    } else if (arg === '--limit') {
      const value = argv[++i];
      if (value === undefined) throw new Error('--limit needs a number');
      args.limit = parseLimit(value);
    } else if (arg?.startsWith('--limit=')) {
      args.limit = parseLimit(arg.slice('--limit='.length));
    } else if (arg === '--cursor-file') {
      const value = argv[++i];
      if (value === undefined) throw new Error('--cursor-file needs a path');
      args.cursorFile = resolve(value);
    } else if (arg?.startsWith('--cursor-file=')) {
      args.cursorFile = resolve(arg.slice('--cursor-file='.length));
    } else if (arg === '--contract') {
      const value = argv[++i];
      if (value === undefined) throw new Error('--contract needs a contract id');
      args.contractId = value;
    } else if (arg?.startsWith('--contract=')) {
      args.contractId = arg.slice('--contract='.length);
    } else if (arg === '-h' || arg === '--help') {
      throw new Error(usage());
    } else {
      throw new Error(`Unknown argument "${arg}".\n\n${usage()}`);
    }
  }

  return args;
}

function parseLedger(value: string, flag: string): number {
  if (!/^\d+$/.test(value)) throw new Error(`${flag} must be a positive ledger number`);
  const ledger = Number(value);
  if (!Number.isSafeInteger(ledger) || ledger < 1) {
    throw new Error(`${flag} must be a positive ledger number`);
  }
  return ledger;
}

function parseLimit(value: string): number {
  if (!/^\d+$/.test(value)) throw new Error('--limit must be a positive number');
  const limit = Number(value);
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 1000) {
    throw new Error('--limit must be between 1 and 1000');
  }
  return limit;
}

function usage(): string {
  return [
    'Usage: npm run events -- [options]',
    '',
    '  --reset                 ignore the saved cursor and start from the latest 1000 ledgers',
    '  --start <ledger>        start from a ledger instead of the saved cursor',
    '  --limit <n>             events per request (1-1000, default 100)',
    '  --cursor-file <path>    where to save the cursor (default examples/.events-cursor.json)',
    '  --contract <id>         contract to watch (default VOUCHER_CONTRACT_ID)',
  ].join('\n');
}

export function cursorMatches(value: unknown, contractId: string): value is CursorState {
  if (value === null || typeof value !== 'object') return false;
  const cursor = value as Partial<CursorState>;
  return (
    cursor.contractId === contractId &&
    typeof cursor.cursor === 'string' &&
    cursor.cursor.length > 0 &&
    typeof cursor.updatedAt === 'string'
  );
}

export function readCursor(file: string, contractId: string): CursorState | null {
  if (!existsSync(file)) return null;
  try {
    const parsed: unknown = JSON.parse(readFileSync(file, 'utf8'));
    if (!cursorMatches(parsed, contractId)) {
      note(`ignoring ${file}: it belongs to a different contract`);
      return null;
    }
    return parsed;
  } catch (err) {
    note(`ignoring unreadable cursor file ${file}: ${errorMessage(err)}`);
    return null;
  }
}

export function writeCursor(file: string, contractId: string, cursor: string): CursorState {
  const state: CursorState = {
    contractId,
    cursor,
    updatedAt: new Date().toISOString(),
  };
  mkdirSync(dirname(file), { recursive: true });
  writeFileSync(file, `${JSON.stringify(state, null, 2)}\n`);
  return state;
}

export function jsonReplacer(_key: string, value: unknown): unknown {
  if (typeof value === 'bigint') return value.toString();
  if (Buffer.isBuffer(value)) return value.toString('hex');
  if (value instanceof Uint8Array) return Buffer.from(value).toString('hex');
  return value;
}

export function decodeEvent(event: RawEventLike): DecodedEvent {
  const topics = event.topic.map((topic) => scValToNative(topic));
  const name = topics.length > 0 ? String(topics[0]) : '(unnamed)';
  return {
    name,
    topics: topics.slice(1),
    value: scValToNative(event.value),
  };
}

export function formatDecodedEvent(event: DecodedEvent): string {
  return JSON.stringify(
    { topics: event.topics, data: event.value },
    jsonReplacer,
    2,
  );
}

function retentionHint(message: string): string | null {
  if (
    /retention|oldestLedger|oldest ledger|startLedger.*(too old|before)|ledger.*too old|outside.*(range|window)|-32001/i.test(
      message,
    )
  ) {
    return (
      'The RPC no longer retains events from that ledger. Re-run with --reset to ' +
      'start from the latest 1000 ledgers, or pass --start <oldest-retained-ledger>.'
    );
  }
  return null;
}

function eventFilter(contractId: string): rpc.Api.EventFilter {
  return { type: 'contract', contractIds: [contractId] };
}

export async function fetchEvents(args: EventsArgs): Promise<number> {
  let cursor = args.reset ? null : readCursor(args.cursorFile, args.contractId)?.cursor ?? null;
  let startLedger = args.startLedger;

  if (args.reset) {
    if (existsSync(args.cursorFile)) rmSync(args.cursorFile);
    note(`reset: ignoring the saved cursor in ${args.cursorFile}`);
  }

  if (cursor === null && startLedger === null) {
    const latest = await rpcServer.getLatestLedger();
    startLedger = Math.max(1, latest.sequence - START_BACKLOG);
    note(`no cursor: starting at ledger ${startLedger} (latest is ${latest.sequence})`);
  }

  let total = 0;
  let page = 0;
  for (;;) {
    page++;
    const request: rpc.Api.GetEventsRequest =
      cursor !== null
        ? { filters: [eventFilter(args.contractId)], cursor, limit: args.limit }
        : {
            filters: [eventFilter(args.contractId)],
            startLedger: startLedger ?? 1,
            limit: args.limit,
          };

    let response: rpc.Api.GetEventsResponse;
    try {
      response = await rpcServer.getEvents(request);
    } catch (err) {
      const message = errorMessage(err);
      const hint = retentionHint(message);
      if (hint) throw new Error(`${message}\n\n${hint}`);
      throw err;
    }

    for (const event of response.events) {
      const decoded = decodeEvent(event);
      console.log(
        `\n[ledger ${event.ledger}] ${decoded.name}  tx ${event.txHash}`,
      );
      console.log(formatDecodedEvent(decoded));
    }

    total += response.events.length;
    if (response.cursor) {
      cursor = response.cursor;
      writeCursor(args.cursorFile, args.contractId, cursor);
    }

    if (response.events.length < args.limit) break;
    if (response.events.length === 0) break;
    if (page > 10_000) throw new Error('stopped after 10,000 pages; the cursor may be looping');
  }

  return total;
}

async function main(): Promise<void> {
  console.log('SCI Healthcare — contract events example (Stellar testnet)');
  const args = parseEventsArgs(process.argv.slice(2));
  say('Configuration');
  note(`contract    ${args.contractId}`);
  note(`cursor file ${args.cursorFile}`);
  note(`rpc         ${RPC_URL}`);

  const total = await fetchEvents(args);
  say(`Decoded ${total} event${total === 1 ? '' : 's'}`);
  note(`cursor saved in ${args.cursorFile}`);
}

const invokedDirectly =
  process.argv[1] !== undefined && pathToFileURL(process.argv[1]).href === import.meta.url;

if (invokedDirectly) {
  main().catch((err) => {
    console.error(`\n${errorMessage(err)}`);
    process.exit(1);
  });
}
