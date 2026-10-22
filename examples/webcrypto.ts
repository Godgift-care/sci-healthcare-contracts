/**
 * Web Crypto for Node 18.
 *
 * The Stellar SDK generates keypairs with `crypto.getRandomValues`. Node 18
 * exposes it as `globalThis.crypto`, but some runners (tsx on 18.x) start
 * without it, and `Keypair.random()` then throws
 * "crypto.getRandomValues must be defined".
 *
 * Imported for its side effect, ahead of the SDK, so the example runs on the
 * Node versions a contributor is likely to have.
 */
import { webcrypto } from 'node:crypto';

if (!globalThis.crypto?.getRandomValues) {
  Object.defineProperty(globalThis, 'crypto', {
    value: webcrypto,
    configurable: true,
  });
}
