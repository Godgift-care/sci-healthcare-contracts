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
  updated_ledger?: number;
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
