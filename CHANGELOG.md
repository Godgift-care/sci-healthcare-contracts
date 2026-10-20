# Changelog

## Unreleased

### Added
- **test:** an authorisation matrix in each of `registry`, `voucher` and `receipt`. A single `AUTH_MATRIX` list holds every public function with the exact signer it must demand, and one table-driven test per contract calls each row and asserts `env.auths()` is exactly that address — no missing `require_auth()`, no extra one. The tables in [the contract reference](docs/developers/contracts.md) are rendered from the same lists, and a second test fails when the docs and the lists disagree. `settle` and `refund` are documented as permissionless, `initialize` as one-shot, and the views as needing no signature.
- **test:** storage TTL behaviour over long periods, in all three contracts. The tests jump the ledger 60–100 days and assert that entries touched by normal use are still readable and are pushed back out to their full 90-day TTL, that the contract instance is extended by writes, and what happens to a voucher nobody touches. Findings are written up under [Storage lifetime](docs/developers/contracts.md#storage-lifetime-ttl): a voucher abandoned for 90 days is restored by the host on the next read rather than lost, and `refund` still returns the funder's money.
- **test:** an event snapshot test for every function that emits an event, in all three contracts. Each reads the decoded XDR and asserts the exact topic list and data map, so renaming a field or moving one between topic and data fails with a message naming `TOPIC_FIELDS` in `sci-healthcare-backend/src/stellar/events.ts`. Covers `initialized`, `voucher_created`, `dispute_resolved`, `admin_proposed`, `admin_changed`, `provider_registered`, `provider_status_changed`, `service_upserted`, `service_removed`, `attester_added`, `attester_removed`, `minter_changed` and `receipt_minted` on top of the five lifecycle events already pinned.
- **ci:** `scripts/e2e-local.sh` and `.github/workflows/e2e-local.yml` deploy, seed and verify against a real Stellar node. The job starts `stellar/quickstart` in local mode, creates the five identities, runs `scripts/deploy.sh` and `scripts/seed.sh`, then reads voucher 1 from the chain and asserts it is `Attested`. It runs on pushes to `main` and on demand, and names the script step that failed. Documented in [Local setup](docs/developers/setup.md#the-end-to-end-check-on-a-real-node).
- **docs:** a [mainnet launch checklist](docs/developers/mainnet-checklist.md) covering code, keys, parameters, monitoring, communication and rollback. Every item names an owner and the evidence required, and it is linked from the README, the docs index and `SECURITY.md`. It records that `scripts/deploy.sh` is testnet-only because it deploys a test USDC asset.
- **docs:** a [multisig admin guide](docs/developers/multisig-admin.md) creates a 2-of-3 Stellar account with the CLI, tests the policy, disables the master key, hands registry, voucher and receipt to it through the two-step handover, and shows how to submit admin calls with two signatures. Linked from the mainnet checklist and the contract reference.
- **docs:** an [integration guide](docs/developers/integration.md) for wallets and remittance apps, with runnable JavaScript for deriving `beneficiary_ref` without leaking identity, quoting the fee, funding a voucher, reading status and reading receipts directly from the contracts, including error handling and a privacy checklist.
- **test:** `multisig_admin_can_act_on_all_three_contracts` hands registry, voucher and receipt to one multisig-style account and performs an admin action on each, covering the handover and the admin calls the guide documents.
- **ci:** a `coverage` job runs `cargo llvm-cov`, prints the coverage table in the job output and fails when the contract sources drop below **98% of lines** or **95% of regions** (today: 98.14% and 95.99%). Test files are excluded from the measurement, so the number says how much of the *contract* code the tests reach. The `lcov` report is uploaded as an artefact, and the README links the job.
- **ci:** a `dependency audit` job runs `cargo deny check` — RustSec advisories, the licence allow list, the duplicate and wildcard bans and the source allow list — against a new `deny.toml`. It carries no exceptions today; any future one has to carry a reason.
- **ci:** a `release` workflow builds the contracts when a `v*` tag is pushed, optimises them with `wasm-opt -Oz` (binaryen 133) and attaches them to the GitHub release with a `SHA256SUMS` file and release notes that explain how to compare the hash with a deployed contract.
- **scripts:** `build-wasm.sh` builds every contract the one way the release, the deployment and the verification agree on, and writes `SHA256SUMS` next to them. `verify-deployments.sh` fetches each deployed contract with `stellar contract fetch` and compares it with a local build twice: by SHA-256, and by contract interface (functions, errors, events) as the Stellar host reads it. It exits non-zero when either differs.
- **docs:** the README publishes the verification result for testnet and explains it. Three of the four deployed contracts expose exactly the interface this source builds; the voucher does not, because its lifecycle events gained `service_code` and `amount` after it was deployed. Redeploying fixes it.
- **docs:** a [glossary of protocol terms](docs/glossary.md) with short definitions and links to where each term is used (attester, claim grace, dispute window, beneficiary reference, basis points and more), linked from [the docs index](docs/SUMMARY.md).
- **docs:** a [clinic FAQ](docs/guides/clinics-faq.md) with thirteen questions covering when clinics are paid, what happens when a patient never comes, how disputes work and what fees apply, each answered with links to the source and checked against the contract constants.
- **docs:** French translations of the funder, clinic and attester guides under [docs/fr/](docs/fr/), with language-switch links at the top of each guide.
- **chore:** [Dependabot configuration](.github/dependabot.yml) for Cargo and GitHub Actions, with weekly updates grouped so soroban-sdk upgrades are labelled for careful review.
- **receipt:** wallet-friendly metadata — `name()`, `symbol()` and `receipt_uri(voucher_id)` read-only views so wallets and explorers can display receipts. The URI carries only the voucher id (no patient data), and receipts are documented as soulbound: there is no transfer function. Closes #59.
- **faucet:** a `daily_cap` in `FaucetConfig`, backed by a shared `(day, minted)` counter that resets at each day boundary; drips beyond the cap are rejected with the new `DailyCapReached` error, so unlimited testnet accounts cannot drain the demo token. Closes #60.
- **faucet:** `set_paused(admin, bool)` lets the admin stop drips without releasing the token admin role; `drip` is rejected while paused. Closes #61.
- **faucet:** `set_drip` now emits a `DripChanged { amount, cooldown }` event so configuration changes are auditable. Closes #62.

### Changed
- **receipt:** `count_for` now extends the TTL of the count entry it reads. It was the only persistent read in the three contracts that did not, so a patient's episode count could be archived while their receipts stayed live. Covered by `count_for_extends_its_own_entry`.
- **scripts:** `scripts/deploy.sh` now refuses to overwrite an existing `deployments/<network>.env` unless `--force` is passed, so an accidental second run cannot replace the live addresses. `--dry-run` prints every command without building, deploying or sending a transaction, and the final summary makes the outcome explicit. `scripts/test-deploy.sh` covers the refusal, `--force` and dry-run paths without a network, and the new `deploy script` CI job runs it on every pull request.
- **deploy:** `scripts/deploy.sh` and `scripts/deploy-faucet.sh` deploy the wasm that `scripts/build-wasm.sh` produces, so what is deployed is what is released. The faucet used to be built on its own (`cargo build -p sci-faucet`), which links a different `soroban-sdk` than the rest of the workspace and produces different bytes.
- **build:** the build takes the cargo registry path out of the wasm (`--remap-path-prefix`). The Soroban SDK embeds the absolute path of its own source in its panic messages, so the artefact used to carry the builder's home directory, and two machines never agreed on its hash.
- **cargo:** the workspace path dependencies carry a version, so `cargo deny` does not read them as wildcard version requirements.
- **voucher:** `voucher_claimed`, `voucher_attested`, `voucher_disputed`, `voucher_settled` and `voucher_refunded` now carry `service_code` and `amount` in their data, appended after the fields each event already had. Only `voucher_created` used to say what the voucher was for, so an indexer or wallet reading a single lifecycle event had to load the voucher to show what was paid for. `voucher_refunded` already carried `amount`; it gains `service_code` only. Topics are unchanged and the data map is keyed by name, so the indexer in `sci-healthcare-backend` decodes the new layout without a breaking change (its matching decoder change is linked from the pull request). Covered by five event snapshot tests in `contracts/voucher/src/test.rs` that read the decoded XDR event and the regenerated `test_snapshots`.

Tests: 144 (was 135).

## 0.2.0 — 2026-10-09

Redeployed to testnet on 2026-10-07; addresses are in `deployments/testnet.env`.

### Fixed
- **voucher:** a `Claimed` voucher that is never attested no longer locks the funder's money forever. `refund` accepts it seven days (`CLAIM_GRACE_SECS`) after expiry.
- **voucher:** a provider suspended after a voucher was funded can no longer `claim` it.
- **voucher:** every state transition extends the instance TTL, not only `create_voucher`.
- **registry:** attester entries are TTL-extended on read like providers and services.

### Added
- "Try it in five minutes" walkthrough in the README.
- Issue forms (bug, feature) and a pull request template; blank issues off, security reports routed to SECURITY.md.
- **faucet** (testnet only): anyone can get 50 test USDC once a day from the app, with no server holding the issuer key. The demo token's SAC names the faucet as admin; `release_token_admin` hands it back. Deployed by `scripts/deploy-faucet.sh`, which refuses mainnet. 10 tests.
- Two-step admin handover (`propose_admin` / `accept_admin` / `get_pending_admin`) in registry, voucher and receipt. Replaces the registry's one-step `set_admin`; the voucher admin previously could not be rotated at all.
- `refundable_at(voucher_id)` view on the voucher contract.
- Documented dispute reason codes 1–4.

Tests: 94 (was 69).

## 0.1.0 — 2026-09-02

Initial registry, voucher escrow and care receipt contracts.
