import test from 'node:test';
import assert from 'node:assert/strict';

import {
  VOUCHER_STATUS,
  decideFromChain,
  dueActionFromApi,
  fetchDueVouchers,
  parseArgs,
  parseUnixTime,
  providerAddress,
  statusName,
  type ApiVoucher,
  type ApiVoucherPage,
} from './keeper.js';

function apiVoucher(overrides: Partial<ApiVoucher> = {}): ApiVoucher {
  return {
    id: '1',
    status: 'Attested',
    ...overrides,
  };
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

test('parseUnixTime accepts the shapes the API and RPC send', () => {
  assert.equal(parseUnixTime('1700000000'), 1_700_000_000);
  assert.equal(parseUnixTime('1700000000000'), 1_700_000_000);
  assert.equal(parseUnixTime('2026-01-01T00:00:00.000Z'), 1_767_225_600);
  assert.equal(parseUnixTime('  1700000000  '), 1_700_000_000);
});

test('parseUnixTime returns null for a missing or unreadable time', () => {
  assert.equal(parseUnixTime(null), null);
  assert.equal(parseUnixTime(undefined), null);
  assert.equal(parseUnixTime(''), null);
  assert.equal(parseUnixTime('   '), null);
  assert.equal(parseUnixTime('not-a-date'), null);
});

test('statusName maps a discriminant and labels an unknown one', () => {
  assert.equal(statusName(VOUCHER_STATUS.Funded), 'Funded');
  assert.equal(statusName(VOUCHER_STATUS.Attested), 'Attested');
  assert.equal(statusName(VOUCHER_STATUS.Refunded), 'Refunded');
  assert.equal(statusName(99), 'unknown(99)');
});

test('providerAddress reads both API shapes', () => {
  assert.equal(providerAddress(apiVoucher({ provider: 'GABC' })), 'GABC');
  assert.equal(providerAddress(apiVoucher({ provider: { address: 'GDEF' } })), 'GDEF');
  assert.equal(providerAddress(apiVoucher()), null);
  assert.equal(providerAddress(apiVoucher({ provider: { name: 'clinic' } })), null);
});

test('an attested voucher is due once the derived flag or its deadline says so', () => {
  const now = 1_000;

  assert.equal(
    dueActionFromApi(apiVoucher({ isSettleable: true, disputeDeadline: null }), now),
    'settle',
  );
  assert.equal(
    dueActionFromApi(
      apiVoucher({ isSettleable: false, disputeDeadline: new Date(900 * 1000).toISOString() }),
      now,
    ),
    'settle',
  );
  assert.equal(
    dueActionFromApi(
      apiVoucher({ isSettleable: false, disputeDeadline: new Date(1_100 * 1000).toISOString() }),
      now,
    ),
    null,
  );
  assert.equal(
    dueActionFromApi(apiVoucher({ isSettleable: false, disputeDeadline: null }), now),
    null,
  );
  assert.equal(
    dueActionFromApi(apiVoucher({ isSettleable: false, disputeDeadline: 'not-a-date' }), now),
    null,
  );
});

test('a funded or claimed voucher is due for refund when its refundable time has passed', () => {
  const now = 1_000;

  assert.equal(
    dueActionFromApi(apiVoucher({ status: 'Funded', isRefundable: true }), now),
    'refund',
  );
  assert.equal(
    dueActionFromApi(
      apiVoucher({ status: 'Funded', isRefundable: false, refundableAt: '900' }),
      now,
    ),
    'refund',
  );
  assert.equal(
    dueActionFromApi(
      apiVoucher({ status: 'Claimed', isRefundable: false, refundableAt: '1100' }),
      now,
    ),
    null,
  );
  assert.equal(
    dueActionFromApi(apiVoucher({ status: 'Claimed', isRefundable: false, refundableAt: null }), now),
    null,
  );
});

test('terminal and unknown statuses are never due', () => {
  const now = 1_000;
  for (const status of ['Settled', 'Disputed', 'Refunded', 'SomethingNew']) {
    assert.equal(dueActionFromApi(apiVoucher({ status }), now), null);
  }
});

test('settle proceeds only for an attested voucher whose window has closed', () => {
  const voucher = {
    status: VOUCHER_STATUS.Attested,
    expires_at: 10_000n,
    dispute_deadline: 1_000n,
  };

  const due = decideFromChain('settle', voucher, null, 1_000);
  assert.equal(due.proceed, true);
  assert.match(due.reason, /dispute window closed/);

  const early = decideFromChain('settle', voucher, null, 999);
  assert.equal(early.proceed, false);
  assert.match(early.reason, /open until/);
});

test('settle is skipped when the chain has already moved on', () => {
  const decision = decideFromChain(
    'settle',
    { status: VOUCHER_STATUS.Settled, expires_at: 10_000n, dispute_deadline: 1_000n },
    null,
    2_000,
  );

  assert.equal(decision.proceed, false);
  assert.match(decision.reason, /Settled, not Attested/);
});

test('a deadline that is not a safe number is skipped rather than guessed', () => {
  const decision = decideFromChain(
    'settle',
    {
      status: VOUCHER_STATUS.Attested,
      expires_at: 10_000n,
      dispute_deadline: BigInt(Number.MAX_SAFE_INTEGER) + 1n,
    },
    null,
    2_000,
  );

  assert.equal(decision.proceed, false);
  assert.match(decision.reason, /JavaScript number/);
});

test('refund proceeds for a funded or claimed voucher once refundable_at has passed', () => {
  const funded = decideFromChain(
    'refund',
    { status: VOUCHER_STATUS.Funded, expires_at: 500n, dispute_deadline: 0n },
    1_000,
    1_000,
  );
  assert.equal(funded.proceed, true);
  assert.match(funded.reason, /refundable since/);

  const claimed = decideFromChain(
    'refund',
    { status: VOUCHER_STATUS.Claimed, expires_at: 500n, dispute_deadline: 0n },
    1_000,
    1_000,
  );
  assert.equal(claimed.proceed, true);
});

test('refund skips an early, missing, or ineligible refundable time', () => {
  const snapshot = {
    status: VOUCHER_STATUS.Funded,
    expires_at: 500n,
    dispute_deadline: 0n,
  };

  const early = decideFromChain('refund', snapshot, 2_000, 1_000);
  assert.equal(early.proceed, false);
  assert.match(early.reason, /not refundable until/);

  const missing = decideFromChain('refund', snapshot, null, 1_000);
  assert.equal(missing.proceed, false);
  assert.match(missing.reason, /returned no time/);
});

test('refund skips a voucher that is no longer refundable', () => {
  const decision = decideFromChain(
    'refund',
    { status: VOUCHER_STATUS.Settled, expires_at: 500n, dispute_deadline: 0n },
    1_000,
    2_000,
  );

  assert.equal(decision.proceed, false);
  assert.match(decision.reason, /cannot be refunded/);
});

test('parseArgs accepts the documented flags and rejects a typo', () => {
  assert.deepEqual(parseArgs([]), { dryRun: false, watch: false, help: false });
  assert.deepEqual(parseArgs(['--dry-run', '--watch']), {
    dryRun: true,
    watch: true,
    help: false,
  });
  assert.deepEqual(parseArgs(['--help']), { dryRun: false, watch: false, help: true });
  assert.throws(() => parseArgs(['--wacth']), /Unknown argument "--wacth"/);
});

test('fetchDueVouchers asks each open status and pages to the total', async () => {
  const pages: Record<string, ApiVoucherPage[]> = {
    Attested: [
      {
        total: 2,
        vouchers: [apiVoucher({ id: '1', isSettleable: true })],
      },
      {
        total: 2,
        vouchers: [apiVoucher({ id: '2', isSettleable: false, disputeDeadline: null })],
      },
    ],
    Funded: [
      {
        total: 1,
        vouchers: [apiVoucher({ id: '3', status: 'Funded', isRefundable: true })],
      },
    ],
    Claimed: [{ total: 0, vouchers: [] }],
  };
  const calls: string[] = [];
  const pageByStatus: Record<string, number> = {};

  const fetchImpl = async (url: string): Promise<Response> => {
    calls.push(url);
    const parsed = new URL(url);
    const status = parsed.searchParams.get('status');
    assert.ok(status !== null);
    const index = pageByStatus[status] ?? 0;
    pageByStatus[status] = index + 1;
    const page = pages[status]?.[index] ?? { total: 0, vouchers: [] };
    return jsonResponse(page);
  };

  const due = await fetchDueVouchers('http://api.test/', 1_000, fetchImpl);

  assert.deepEqual(
    due.map((entry) => `${entry.voucher.id}:${entry.action}`),
    ['1:settle', '3:refund'],
  );
  assert.equal(calls.length, 4);
  assert.match(calls[0] ?? '', /status=Attested&limit=100&offset=0$/);
  assert.match(calls[1] ?? '', /status=Attested&limit=100&offset=1$/);
  assert.match(calls[2] ?? '', /status=Funded&limit=100&offset=0$/);
  assert.match(calls[3] ?? '', /status=Claimed&limit=100&offset=0$/);
});

test('fetchDueVouchers throws when the backend is unhealthy', async () => {
  const fetchImpl = async (): Promise<Response> =>
    new Response('database unavailable', { status: 503 });

  await assert.rejects(
    () => fetchDueVouchers('http://api.test', 1_000, fetchImpl),
    /GET \/vouchers\?status=Attested failed: 503 database unavailable/,
  );
});

test('fetchDueVouchers rejects a response that is not a voucher page', async () => {
  const fetchImpl = async (): Promise<Response> => jsonResponse({ error: 'filter_required' });

  await assert.rejects(
    () => fetchDueVouchers('http://api.test', 1_000, fetchImpl),
    /returned no vouchers array/,
  );
});
