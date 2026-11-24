# Contributing to SCI Healthcare Contracts

Thanks for helping. This repo holds the Soroban contracts, so changes here move
real escrowed value — the bar is deliberately high.

By taking part you agree to the [Code of Conduct](CODE_OF_CONDUCT.md).

## Before you start

1. Comment on the issue you want and wait to be assigned, so two people do not
   build the same thing.
2. If you are proposing a design change rather than fixing a defect, open an
   issue first and agree the approach before writing code.

## Setup

```bash
rustup toolchain install 1.96.0
rustup target add wasm32v1-none
cargo install --locked stellar-cli
cargo test
```

The toolchain is pinned in `rust-toolchain.toml`. Please do not change it in a
feature PR.

### Minimum supported Rust version

The workspace declares `rust-version = "1.96"` in the `[workspace.package]`
section of [`Cargo.toml`](Cargo.toml), and every contract inherits it. That is
the same version `rust-toolchain.toml` pins and the version the CI `msrv` job
builds with, so downstream crates that depend on the contract types can read
the requirement from `cargo metadata` instead of finding it out from a build
failure.

Raising it is a deliberate change, never a side effect of a feature:

- Open its own pull request, say in the description what needs the newer
  compiler (a dependency or a language feature), and add a `CHANGELOG.md` entry.
- Bump `rust-version` and the `rust-toolchain.toml` channel in the same change,
  plus the dev container's `RUST_VERSION` and the toolchain in the CI `msrv`
  job. The release build has to keep working, so the pins move together.
- The `msrv` job must pass on the new version, and its first step fails if the
  declared version and the toolchain it runs have drifted apart.

The version is only lowered when the workspace really does compile there; the
`msrv` job is the proof, so a lower number without that job passing is not a
supported claim.

### Dev container

The repository ships a [dev container](.devcontainer/) with everything pinned
to the versions CI uses: Rust 1.96.0 with the `wasm32v1-none` target, the
Stellar CLI 27.1.0, `wasm-opt` from binaryen 133 and `just`. Open the
repository in VS Code with the Dev Containers extension (**Reopen in
Container**) or in GitHub Codespaces, and `cargo test` and `just wasm` work
straight away. When CI bumps one of those versions, bump it in
`.devcontainer/Dockerfile` in the same change.

### Common tasks

A [`justfile`](justfile) wraps the commands CI and the docs use, so there is
one entry point. Install [`just`](https://github.com/casey/just) (it is already
in the dev container), then:

| Recipe | Runs |
| --- | --- |
| `just` | lists the recipes |
| `just build` | `cargo build --all` |
| `just test` | `cargo test --all` |
| `just fmt` / `just fmt-check` | `cargo fmt --all` / with `-- --check` |
| `just clippy` | `cargo clippy --all-targets --all-features -- -D warnings` |
| `just check` | `fmt-check`, `clippy` and `test` — what CI runs on a pull request |
| `just coverage` | the CI `coverage` job's check: `cargo llvm-cov --all --summary-only`, test files excluded, failing under 98% of lines or 95% of regions (needs `cargo-llvm-cov` 0.9.1 and the `llvm-tools-preview` component) |
| `just doc` | the CI `doc` job's build: `cargo doc --no-deps --workspace --document-private-items`, into `target/doc` |
| `just snapshots` | `cargo test --all`, which rewrites `contracts/*/test_snapshots/`, then lists the snapshot files that changed; commit them, or the CI `test` job fails |
| `just wasm [out]` | `scripts/build-wasm.sh` (default `dist`) |
| `just deploy [network] [flags]` | `scripts/deploy.sh`, e.g. `just deploy testnet --dry-run` |
| `just seed [network]` | `scripts/seed.sh` |
| `just verify [network]` | `scripts/verify-deployments.sh` |
| `just test-deploy` | `scripts/test-deploy.sh` |
| `just e2e-local` | `scripts/e2e-local.sh` against a local node on port 8000 |
| `just hooks` / `just unhooks` | turn the opt-in [git hooks](#git-hooks-optional) on or off |

The network defaults to `testnet`; set `NETWORK` to change it.

### Git hooks (optional)

Formatting and lint failures otherwise show up in CI minutes after a push.
Two opt-in hooks in [`.githooks/`](.githooks/) run the same checks locally:

| Hook | Runs | Same as |
| --- | --- | --- |
| `pre-commit` | `cargo fmt --all -- --check`, only when the commit touches a `.rs` file | the CI `fmt` job |
| `pre-push` | `cargo clippy --all-targets --all-features -- -D warnings` | the CI `clippy` job |

Nothing runs until you turn them on, per clone:

```bash
just hooks      # git config core.hooksPath .githooks
just unhooks    # back to git's default hooks directory
```

- **Windows.** Git for Windows runs these hooks with its bundled `sh`, so they
  work from Git Bash, PowerShell, `cmd` and GUI clients, as long as `cargo` is
  on `PATH` (rustup adds it). `just` runs its recipes with `bash`; without Git
  Bash on `PATH`, run `git config core.hooksPath .githooks` directly instead of
  `just hooks`.
- **Dev container and Codespaces.** They work there as they are: the container
  has `cargo` and `just`. The setting is per clone, so a codespace needs
  `just hooks` once.
- The format check reads the files on disk, not only the staged ones, so stage
  the result of `cargo fmt --all` before committing again.
- A hook that cannot find `cargo` prints a note and lets the commit or push
  through. Skip a hook once with `git commit --no-verify` or
  `git push --no-verify`.

## Non-negotiables

These are the rules a review will fail you on.

- **No patient health information on chain, ever.** Service codes stay coarse
  categories. `beneficiary_ref` stays an opaque 32 bytes. If a change would let
  anyone infer a diagnosis, a result, or an identity from ledger data, it will
  be rejected regardless of how useful it is.
- **No `unwrap()` or `expect()` outside `#[cfg(test)]`.** Return a
  `#[contracterror]` variant instead.
- **No floating point.** Money is `i128` in the token's smallest unit. Fees are
  basis points with integer division, and truncation must favour the provider,
  never the protocol.
- **Use checked arithmetic** on anything that could overflow. `MathOverflow`
  exists for this.
- **Extend TTL on every persistent read and write.** An entry that expires is an
  entry that has silently lost user funds.
- **`require_auth()` on the acting address**, and check role membership
  separately. Authenticating is not the same as being authorised.
- **Every new function needs tests for the failure paths**, not just the happy
  one. Look at how existing tests assert specific error variants.

## Testing

`cargo test` runs every contract's tests. The tests live in each contract's
`src/test.rs` and use the Soroban test environment, so a failure names the
contract call and the assertion rather than needing a running network.

### Fixtures

Each contract has a `setup()` that builds the same state a deployment has.
The voucher fixture registers the token, registry, receipt book and voucher,
then wires them together in the order `scripts/deploy.sh` uses:

```rust
let env = Env::default();
env.mock_all_auths();
env.ledger().with_mut(|li| li.timestamp = START_TIME);

let issuer = Address::generate(&env);
let sac = env.register_stellar_asset_contract_v2(issuer);
let token_addr = sac.address();

let registry_addr = env.register(Registry, ());
let registry = RegistryClient::new(&env, &registry_addr);
registry.initialize(&admin);

let voucher_addr = env.register(VoucherEscrow, ());
let voucher = VoucherEscrowClient::new(&env, &voucher_addr);

let receipt_addr = env.register(ReceiptBook, ());
let receipts = ReceiptBookClient::new(&env, &receipt_addr);
receipts.initialize(&admin, &voucher_addr);

voucher.initialize(
    &admin,
    &registry_addr,
    &receipt_addr,
    &token_addr,
    &DISPUTE_WINDOW,
    &FEE_BPS,
    &fee_account,
);
```

Keep generated addresses in the fixture and reuse them. Address generation
depends on call order, so adding an `Address::generate` in `setup()` changes
every snapshot that test writes; the authorisation-matrix tests deliberately
generate their extra nominee in their own fixture for this reason.

### Time travel

`timestamp` is ledger time in seconds; `sequence_number` is the ledger count
used by TTL tests. Move them independently, and only as far as the test needs:

```rust
// Advance three days: a dispute window closes, but the voucher TTL does not.
env.ledger().with_mut(|li| {
    li.timestamp += 3 * 24 * 60 * 60;
});

// Advance sixty days of ledgers to exercise TTL extension.
env.ledger().with_mut(|li| {
    li.sequence_number += 60 * 17_280;
    li.timestamp += 60 * 24 * 60 * 60;
});
```

### Authentication

`env.mock_all_auths()` makes every `require_auth()` succeed, which is useful
for setting up state but hides a missing `require_auth()`. Assert what the
last invocation actually demanded:

```rust
let auths = env.auths();
assert_eq!(auths.len(), 1);
assert_eq!(auths[0].0, funder);
```

The `authorisation_matrix_matches_every_public_function` test is built on
this: one table row per public function, with the exact signer it must demand.
A missing `require_auth()` lets anyone move another party's money; an extra
one turns a permissionless `settle` into a call that needs a signature nobody
should have to collect.

### Events

Read the events the invocation published, filtered to the contract:

```rust
use soroban_sdk::testutils::Events as _;

let events = env.events().all().filter_by_contract(&voucher.address);
assert_eq!(events.events().len(), 1);
```

Event payloads are part of the public interface. The `*_event_snapshot` tests
decode the XDR and assert the exact topic list and data map, so renaming a
field or moving one between topic and data fails instead of silently breaking
the indexer. Regenerate the files under `contracts/*/test_snapshots/` only
when the change is intentional, and update `TOPIC_FIELDS` in
`sci-healthcare-backend/src/stellar/events.ts` in the same change set.

### Errors

Every fallible entry point has a generated `try_` method. It returns the
host result and then the contract's typed error, so two unwraps reach the
variant:

```rust
let err = voucher.try_refund(&id).err().unwrap().unwrap();
assert_eq!(err, VoucherError::NotYetExpired);
```

Prefer this to `#[should_panic]`: it pins the error number callers decode and
keeps the failure path readable. Test each new error variant, including the
input validation, authorisation and state checks, not only the happy path.

## Forking, Codespaces and pushing

Only maintainers can push to this repository, and `main` takes changes through
pull requests only. If `git push` answers `403` or `Permission denied`, the
push is going to `Godgift-care/sci-healthcare-contracts` instead of your fork.

1. Fork the repository, then clone **your fork**
   (`gh repo fork Godgift-care/sci-healthcare-contracts --clone` does both).
2. Create a branch, commit, and `git push -u origin <branch>`.
3. Open a pull request against `main`.

**GitHub Codespaces.** The repository has a [dev container](.devcontainer/), so
a codespace starts with the toolchain CI uses and the dependencies installed.
Create it from your fork (**Code → Codespaces** on your fork's page) and
`git push` works as usual. A codespace created from this repository without
write access cannot push here; GitHub offers to fork it for you on the first
push, or run `gh repo fork --remote` and push to the new `origin`.

A codespace cannot push changes to `.github/workflows/`: its token has no
workflow permission. Make those changes from a local clone.

Never commit a `.env` file, and never commit files you did not write. If
`git status` shows changes you do not recognise (an `init` file, edited
`package.json` scripts, a changed `.gitignore`, config files with one very long
line), stop and check your machine before pushing: see
[Security scan](#security-scan).

## Commits

Conventional commits, one logical change per commit:

```
feat(voucher): add partial refund for over-funded vouchers
fix(registry): reject a two-character country code containing digits
test(receipt): cover minting after a minter rotation
docs(readme): correct the deploy order
```

Types: `feat`, `fix`, `test`, `docs`, `refactor`, `chore`, `perf`.

## Pull requests

- Branch from `main`, named `feat/short-description` or `fix/short-description`.
- `cargo test` and `cargo build --target wasm32v1-none --release` must both pass.
- `cargo fmt --all` and `cargo clippy --all-targets -- -D warnings` must be clean.
- Describe **what breaks if this is wrong**, not just what you changed.
- GitHub requests a review from the owners in
  [`.github/CODEOWNERS`](.github/CODEOWNERS) for every contract crate,
  `scripts/` and `.github/` path a pull request touches.
- If anything under `contracts/` changes, add an entry under `## Unreleased` in
  `CHANGELOG.md`. The `changelog entry` check fails without one. A change no
  integrator can notice (a comment, a test-only refactor) can skip it: ask a
  maintainer to add the `skip-changelog` label, and the check re-runs.
- If you changed an event's fields, say so explicitly — the indexer in
  `sci-healthcare-backend` decodes events by name, and moving a field between
  topic and data is a breaking change that needs a matching PR there.

## Documentation site

The [docs site](https://godgift-care.github.io/sci-healthcare-contracts/) is
built from `docs/` by `.github/workflows/docs-site.yml`. The root shows `main`,
and every release tag gets a frozen copy under `/<tag>/` with a banner saying
which version it is, so a page you change on `main` does not rewrite the docs of
a release that is already deployed. Pull requests that touch `docs/` build the
whole site, so a broken page fails before merge.

To build it locally (Ruby 3.1 or newer):

```bash
git fetch --tags upstream          # the snapshots are read from the tags
BUNDLE_GEMFILE=scripts/docs-site/Gemfile bundle install
DOCS_BASE_PATH="" ./scripts/build-docs-site.sh _site
python3 -m http.server 4000 --directory _site   # http://localhost:4000, /v0.2.0/, /latest/
```

`DOCS_BASE_PATH=""` serves the site from `/` for local preview; CI sets the
real path. `DOCS_TAGS=""` builds `main` only.

### Translations

The funder, clinic and attester guides and the protocol pages also exist in
French (`docs/fr/`) and Swahili (`docs/sw/`). The funder, clinic and attester
guides also exist in Hausa (`docs/ha/`), Arabic (`docs/ar/`, rendered right to
left) and Amharic (`docs/am/`), each with a `glossary.md` that fixes how technical
terms are translated. A translated page declares its
counterparts in front matter, and the switcher in `docs/_includes/header_custom.html`
renders them at the top of the page:

```yaml
language: fr
translations:
  en: /guides/funders.html
  sw: /sw/funders.html
```

Link every counterpart that exists, and each one has to link back.
`node .github/scripts/i18n-check.cjs docs` fails when a translated page has no
front matter, points at a page that does not exist, or omits a link; CI runs
it, and `node --test .github/scripts/i18n-check.test.cjs` covers the check.

## Adding a contract event

Events are part of the public interface. When adding or changing one:

1. Define it with `#[contractevent]` in the contract's `events.rs`.
2. Mark fields `#[topic]` only if they should be indexable. Topic fields are
   published in the topic list, **not** in the data map.
3. Update `TOPIC_FIELDS` in `sci-healthcare-backend/src/stellar/events.ts` in the
   same change set, and open a linked PR there.

## How pull requests are merged

A bot merges pull requests automatically (squash) as soon as every CI check on the latest commit has passed, provided the pull request:

- is not a draft and has no `do-not-merge` label,
- links the issue you were assigned on Drips Wave, with a line such as `Closes #123` in the description (Wave points are awarded when that issue is closed as completed, which the bot does on merge),
- has no merge conflicts and is up to date with `main` (the bot comments if it is behind),
- passes the security scan (the `security-scan` status, see below),
- and, if it changes anything under `.github/` (CI workflows and repository automation), has been approved by a maintainer. The bot comments with the files that need approval.

So open your pull request as a draft until it is ready, and keep CI green. To stop an automatic merge, add the `do-not-merge` label or convert the pull request back to a draft.

### Security scan

Every pull request is scanned for malicious changes before it can be merged. The scan runs from `main`, reads your changes as data and never runs them. It reports on the pull request as the `security-scan` status and, if anything needs attention, in a comment listing each finding.

- **block** findings are never merged automatically. Examples are executables and unexpected binaries, hidden or bidirectional Unicode characters (Trojan Source), crypto miners, reverse shells, piping a download into a shell, reading credential files, dependencies from git URLs, tarballs or paths outside the repository, look-alike (typosquatted) package names, packages that do not exist on npm, lockfile entries from another registry or without an integrity hash, crates from outside crates.io, and packages listed as malware in the OSV database.
- **review** findings are common in legitimate work but also how malware gets in. Examples are new dependencies that are recent or rarely used, changed npm scripts, `eval` and spawned processes, encoded or minified code, new network hosts or Stellar addresses, shell scripts, and security-sensitive files. A maintainer reads the change and adds the `security-reviewed` label, after which it merges as usual. A new push removes the label, so the latest commit is always the one reviewed.

If a finding is a false positive, say so on the pull request; please do not try to reword code to get past the scan.

## Wave program

Issues carry a complexity label that maps to Wave points: `trivial` (100),
`medium` (150), `high` (200). Please read the
[Wave rules](https://docs.drips.network/wave/terms-and-rules/) — in particular,
low-effort or unreviewed LLM-generated submissions are explicitly disallowed and
will be closed. Understand what you submit and test it.
