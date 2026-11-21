import { Keypair } from '@stellar/stellar-sdk';

import { addr, i128, str, u32 } from './args.js';
import { SERVICE_CODE, SERVICE_LABEL, SERVICE_PRICE } from './config.js';
import { record } from './output.js';
import { invoke, read, tryRead } from './rpc.js';

export async function ensureRegistryRoles(
  admin: Keypair,
  clinic: Keypair,
  attester: Keypair,
  registry: string,
): Promise<void> {
  const clinicAddr = clinic.publicKey();
  const attesterAddr = attester.publicKey();

  const existing = await tryRead(registry, 'get_provider', [addr(clinicAddr)]);
  if (existing === null) {
    const { hash } = await invoke(
      registry,
      'register_provider',
      [addr(clinicAddr), str('Example Clinic'), str('NG')],
      clinic,
      'register',
    );
    record('register', hash);
  }

  const active = await read<boolean>(registry, 'is_active_provider', [addr(clinicAddr)]);
  if (!active) {
    const { hash } = await invoke(
      registry,
      'set_provider_status',
      [addr(admin.publicKey()), addr(clinicAddr), u32(1), u32(1)],
      admin,
      'activate provider',
    );
    record('activate', hash);
  }

  const price = await read<bigint>(registry, 'get_service_price', [
    addr(clinicAddr),
    u32(SERVICE_CODE),
  ]);
  if (price === 0n) {
    const { hash } = await invoke(
      registry,
      'upsert_service',
      [addr(clinicAddr), u32(SERVICE_CODE), str(SERVICE_LABEL), i128(SERVICE_PRICE)],
      clinic,
      'list service',
    );
    record('service', hash);
  }

  const isAttester = await read<boolean>(registry, 'is_attester', [addr(attesterAddr)]);
  if (!isAttester) {
    const { hash } = await invoke(
      registry,
      'add_attester',
      [addr(admin.publicKey()), addr(attesterAddr)],
      admin,
      'add attester',
    );
    record('attester', hash);
  }
}
