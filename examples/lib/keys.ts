import { Keypair } from '@stellar/stellar-sdk';

export function requiredKey(envName: string, hint: string): Keypair {
  const secret = process.env[envName];
  if (!secret) throw new Error(`Missing ${envName}. ${hint}`);
  return Keypair.fromSecret(secret);
}

/**
 * Uses the given secret, or generates one and prints it — the script
 * friendbots whatever it creates, so a first run needs only ADMIN_SECRET.
 */
export function keyOrCreate(envName: string, role: string): Keypair {
  const secret = process.env[envName];
  if (secret) return Keypair.fromSecret(secret);
  const kp = Keypair.random();
  console.log(`    generated ${envName} for this run — export it to reuse the ${role}:`);
  console.log(`      export ${envName}=${kp.secret()}`);
  return kp;
}
