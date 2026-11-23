import { Address, nativeToScVal, xdr } from '@stellar/stellar-sdk';

export const addr = (a: string): xdr.ScVal => Address.fromString(a).toScVal();
export const u32 = (v: number): xdr.ScVal => nativeToScVal(v, { type: 'u32' });
export const u64 = (v: bigint | number): xdr.ScVal => nativeToScVal(BigInt(v), { type: 'u64' });
export const i128 = (v: bigint): xdr.ScVal => nativeToScVal(v, { type: 'i128' });
export const str = (v: string): xdr.ScVal => nativeToScVal(v, { type: 'string' });

/** BytesN<32> is raw bytes, not a hex string (see scripts/seed.sh). */
export const bytes32 = (hex: string): xdr.ScVal => xdr.ScVal.scvBytes(Buffer.from(hex, 'hex'));
