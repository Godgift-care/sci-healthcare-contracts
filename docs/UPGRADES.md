# Contract Upgrades

Soroban supports upgrading a contract's WASM executable in place while preserving its instance and persistent storage state. This document describes the upgrade procedure, access controls, storage versioning, migration hooks, risks, rollback strategy, and user communication for the SCI Healthcare contracts (`registry`, `voucher`, `receipt`).

---

## 1. Access Control & Authorization

- **Admin Only**: Upgrades are strictly admin-gated. Calling `upgrade(admin, new_wasm_hash)` requires authorization from the stored administrator (`admin.require_auth()`).
- Any attempt by a non-admin or unauthorized account to call `upgrade` reverts immediately with `NotAuthorized`.
- When an upgrade takes place, the contract emits an `Upgraded` event carrying the `new_wasm_hash`:
  ```rust
  pub struct Upgraded {
      pub new_wasm_hash: BytesN<32>,
  }
  ```

---

## 2. Storage Versioning & Post-Upgrade Migration

- **StorageVersion Key**: Each upgradable contract stores its schema version in instance storage under `DataKey::StorageVersion`.
- **Version Tracking**: `get_storage_version(env)` returns the current layout version (`0` if unset / unmigrated).
- **Migration Hook (`migrate`)**:
  - Callable only by the admin (`admin.require_auth()`).
  - Increments and records the new version in instance storage (`new_version = current_version.saturating_add(1)`).
  - Can apply deterministic, idempotent schema transformations or initializations required by the new contract code.

---

## 3. Recommended Upgrade Procedure

1. **Deterministic Build & Verification**:
   - Build the replacement WASM using `./scripts/build-wasm.sh dist`.
   - Verify that all unit tests, clippy checks, and snapshot tests pass on the new codebase.
   - Record the SHA256 hash of the generated `.wasm` binary.

2. **Testnet Rehearsal**:
   - Deploy the new WASM byte-code to Stellar Testnet via `stellar contract install --wasm <path>`.
   - Call `upgrade` on a testnet instance of the contract using the testnet admin account.
   - Execute `migrate` if storage changes were introduced.
   - Verify post-upgrade state reads (e.g., active clinics, open vouchers, minted receipts) to ensure backward compatibility.

3. **Ledger Upload on Target Network**:
   - Install the new WASM onto the network using `stellar contract install --wasm <path>` or an authorized deployer script to obtain `new_wasm_hash`.

4. **Execute Upgrade**:
   - Invoke `upgrade(admin, new_wasm_hash)`.
   - Invoke `migrate(admin)` immediately after if a data migration is needed.

---

## 4. Risks & Mitigations

| Risk | Description | Mitigation |
| --- | --- | --- |
| **Storage Layout Corruption** | Changes to existing `#[contracttype]` field orders or types can cause deserialization failures on existing entries. | Never reorder or change types of existing fields in persistent storage. Add new fields wrapped in `Option` or under new `DataKey` entries. Verify against stored type fixtures (`stored_type_fixtures_decode_against_current_code`). |
| **WASM Upload Mismatch** | Passing an uninstalled or incorrect WASM hash causes the invocation to fail or points to erroneous logic. | Always compute and double-check WASM hashes before submitting the `upgrade` call. Test first on Testnet. |
| **Unauthorized Upgrade Attempt** | Rogue actor attempts to hijack contract code. | All contracts enforce two-step admin handover and strict `require_auth()` checks on `upgrade`. |

---

## 5. Rollback Procedure

Soroban contract upgrades do not automatically retain historical code versions on chain, but the previous WASM remains installed in the ledger until its TTL expires.
- If a critical bug is discovered immediately following an upgrade, the admin can call `upgrade(admin, previous_wasm_hash)` with the prior known-good WASM hash.
- If `migrate` was executed and wrote state incompatible with the old version, compensation logic or a targeted patch WASM must be applied.

---

## 6. User and Integrator Communication

1. **Pre-Upgrade Notice**:
   - Post an advisory in the repository issues / discussions and official notification channels detailing the upgrade motivation, changed behavior, and target ledger timestamp.
2. **Event Indexing**:
   - Off-chain indexers and explorers observe the `Upgraded { new_wasm_hash }` event to recognize that the contract code has been updated.
3. **Changelog & Documentation**:
   - Document any new entry points, modified validation rules, or deprecations in `CHANGELOG.md` and `docs/developers/contracts.md`.
