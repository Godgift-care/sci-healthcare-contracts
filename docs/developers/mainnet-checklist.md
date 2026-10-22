---
title: Mainnet launch checklist
nav_order: 13
---

# Mainnet launch checklist

Mainnet is a reviewed decision, not a script run. Every item below has an
**owner** and a piece of **evidence** that a reviewer can inspect. An item is
only complete when the evidence is attached to the launch pull request or to
the launch issue; "it worked on my machine" is not evidence.

The contracts are **unaudited** and the code in this repository has only been
exercised on testnet. Until every gate below is signed off, do not deploy to
mainnet and do not use real funds. See [SECURITY.md](../../SECURITY.md).

## How to use this checklist

1. Copy the sign-off table into the launch issue for the exact commit being
   deployed.
2. Assign a named person to every owner role. A role without a name is not an
   owner.
3. Attach the evidence as a link, command output, transaction hash, screenshot
   or signed statement. Record the date.
4. Re-check any item whose code, keys, parameters or infrastructure changed
   after it was signed off.
5. The launch is approved only when every item is checked and the final
   sign-off below is complete.

## Owners

| Role | Responsible for |
| --- | --- |
| Contracts maintainer | Code, build, release, contract behaviour |
| Security reviewer | Audit, threat model, key custody review |
| Release manager | Tags, artefacts, deployment record, launch sequence |
| Operations lead | Network access, monitoring, on-call, rollback drills |
| Communications lead | User notices, status page, incident contacts |
| Incident commander | Go/no-go decision and coordination during an incident |

## 1. Code

| # | Requirement | Owner | Evidence needed | Done |
| --- | --- | --- | --- | --- |
| 1.1 | A third-party audit of the registry, voucher, receipt and faucet contracts is complete, and every critical/high finding is fixed or explicitly accepted in writing. | Security reviewer | Audit report link, finding tracker, and a statement for each accepted risk. | [ ] |
| 1.2 | The commit being launched is frozen. No contract, dependency or build-script changes land after the release tag. | Contracts maintainer | `git rev-parse HEAD` and `git status --porcelain` (clean), recorded in the launch issue. | [ ] |
| 1.3 | Formatting, linting, tests, coverage and dependency audit are green on that exact commit. | Contracts maintainer | CI run URL showing `fmt`, `clippy`, `test`, `coverage`, `dependency audit` and `build-wasm` passing. | [ ] |
| 1.4 | A release tag exists and its `SHA256SUMS` matches the artefacts built from the frozen commit. | Release manager | Release URL, tag name, `SHA256SUMS` output and the local `sha256sum dist/*.wasm` output. | [ ] |
| 1.5 | The deployed wasm matches the release and exposes the expected interface. | Release manager | `./scripts/verify-deployments.sh mainnet` output showing every contract matches by hash and interface. | [ ] |
| 1.6 | The contract addresses, token address and admin address are recorded in the launch issue and in `deployments/mainnet.env`. | Release manager | Committed or securely archived env file and the corresponding Stellar Expert links. | [ ] |
| 1.7 | `scripts/deploy.sh` has not been used as-is on mainnet. It deploys a test USDC asset from `$ISSUER`; the real mainnet token must be an existing, verified USDC Stellar Asset Contract. | Contracts maintainer | The reviewed mainnet deployment procedure, or a written statement that deployment was done with a separately reviewed process. | [ ] |
| 1.8 | The faucet is not deployed on mainnet and cannot mint the production token. `scripts/deploy-faucet.sh` refuses mainnet. | Contracts maintainer | Deployment record has no `FAUCET_CONTRACT_ID`; token admin is checked on chain. | [ ] |

## 2. Keys

| # | Requirement | Owner | Evidence needed | Done |
| --- | --- | --- | --- | --- |
| 2.1 | The admin of registry, voucher and receipt is a Stellar account with a **2-of-3 multisig** policy, not a single key. | Security reviewer | `get_admin` / `get_config` output for all three contracts, plus the Horizon account record showing signers and thresholds. Follow [Using a multisig admin](multisig-admin.md). | [ ] |
| 2.2 | All three signers are held by different people, on separate hardware wallets or offline machines, in separate physical locations. | Operations lead | Signed custody attestation naming the three signers, their hardware, and their storage locations. Do not record secret keys. | [ ] |
| 2.3 | A recovery copy of every signer seed or hardware backup exists, is sealed, and has been restored once in a rehearsal. | Operations lead | Sealed-backup register and the date/location of the restore rehearsal. | [ ] |
| 2.4 | Admin handover was done through `propose_admin` then `accept_admin`, and the old deployer key is no longer admin on any contract. | Contracts maintainer | Transaction hashes for every propose/accept call, and post-handover `get_admin` output. | [ ] |
| 2.5 | The token issuer key is not used as a contract admin and its custody is documented. The production token admin is the correct entity (not the faucet). | Security reviewer | Issuer account record, token admin lookup, and a custody statement. | [ ] |
| 2.6 | The fee account is controlled by the same reviewed custody policy as the admin, or by a documented treasury process. | Operations lead | Account address in `get_config` and the treasury/custody approval record. | [ ] |
| 2.7 | Deployer, issuer, fee and admin identities are separated; no single person can deploy, configure and resolve disputes alone. | Security reviewer | Identity-to-role table and written approval of the separation. | [ ] |
| 2.8 | Every key has an owner, a rotation date and an emergency revocation procedure. | Operations lead | Key register with owners and dates, plus the revocation runbook. | [ ] |

## 3. Parameters

| # | Requirement | Owner | Evidence needed | Done |
| --- | --- | --- | --- | --- |
| 3.1 | `token` is the verified mainnet USDC Stellar Asset Contract, not a test asset or a newly issued asset. | Contracts maintainer | `get_config` output and the token contract id cross-checked against the issuer's published mainnet asset. | [ ] |
| 3.2 | `registry`, `receipt_book` and `token` in `get_config` point at the intended mainnet contracts. | Contracts maintainer | `stellar contract invoke ... get_config` output for the voucher and the receipt book's `get_minter` output. | [ ] |
| 3.3 | `fee_bps` is the approved rate (currently 1% in the deployment default), is no more than 1000, and is disclosed to users. | Communications lead | `get_config` output, the product/fee approval, and a copy of the user-facing fee disclosure. | [ ] |
| 3.4 | `fee_account` is the approved treasury address. | Operations lead | `get_config` output and the treasury approval record. | [ ] |
| 3.5 | `dispute_window` is the approved duration (currently 72 hours) and matches the UI copy and operations rota. | Communications lead | `get_config` output, UI copy review, and on-call coverage for the window. | [ ] |
| 3.6 | `quote(amount)` returns the expected `(fee, net)` for representative amounts, including a 1-unit amount and a large amount. | Contracts maintainer | Recorded quote output and the expected integer arithmetic calculation. | [ ] |
| 3.7 | Provider and attester onboarding criteria are agreed, and the initial provider/attester list is reviewed. | Operations lead | Signed onboarding policy and the initial on-chain registration list. | [ ] |
| 3.8 | The receipt book's minter is the voucher contract, and only that contract can mint. | Contracts maintainer | `get_minter` output and a negative test showing another address cannot mint. | [ ] |
| 3.9 | All three contracts are initialized exactly once with the intended admin and no pending admin is left over. | Release manager | Initialization transaction hashes, `get_pending_admin` returning `None`, and `get_admin` output. | [ ] |
| 3.10 | `attester_share_bps` is the approved attester compensation, is no greater than `fee_bps`, and the resulting clinic net and attester payment are documented in the fee disclosure. | Communications lead | `get_config` output, the share/fee approval, and a worked settlement example from `quote` and `voucher_settled`. | [ ] |

## 4. Monitoring

| # | Requirement | Owner | Evidence needed | Done |
| --- | --- | --- | --- | --- |
| 4.1 | The indexer is running against mainnet, caught up to the chain tip, and `/ready` reports healthy. | Operations lead | `/ready` response showing `status: ready`, current ledger and lag below the alert threshold. | [ ] |
| 4.2 | Alerts exist for indexer lag, RPC errors, failed transactions and a stopped indexer. | Operations lead | Alert rule links and a test alert delivered to the on-call channel. | [ ] |
| 4.3 | Alerts exist for unexpected admin events (`admin_changed`, `admin_proposed`), provider suspensions and attester changes. | Security reviewer | Alert rule links and a test event that triggered a notification. | [ ] |
| 4.4 | Settlement, refund and dispute volume are dashboarded, with an alert for anomalous spikes or escrow balance drift. | Operations lead | Dashboard link and the reconciliation query/output for the current escrow balance. | [ ] |
| 4.5 | A daily reconciliation compares token balances held by the voucher contract with open vouchers and flags any difference. | Contracts maintainer | Reconciliation script/query, the first run output, and the expected-zero-difference record. | [ ] |
| 4.6 | Logs are retained, access-controlled and contain no beneficiary identifiers or secret keys. | Security reviewer | Log retention policy, access list and a sample log review. | [ ] |
| 4.7 | On-call coverage is assigned for launch day and the first two weeks, including the dispute-resolution window. | Operations lead | On-call rota with names, time zones and escalation paths. | [ ] |
| 4.8 | Monitoring has been rehearsed against testnet, including a deliberately failed transaction and a lag alert. | Operations lead | Rehearsal date, test transaction hash and notifications received. | [ ] |

## 5. Communication

| # | Requirement | Owner | Evidence needed | Done |
| --- | --- | --- | --- | --- |
| 5.1 | Incident contacts are written down and shared with every owner: internal escalation, hosting/RPC provider, wallet partner and legal/compliance. | Communications lead | Contact sheet in the incident runbook, with a named primary and backup for each. | [ ] |
| 5.2 | A public security contact is live and monitored (`SECURITY.md`), with a 72-hour acknowledgement commitment. | Communications lead | Link to `SECURITY.md` and the mailbox/queue that receives reports. | [ ] |
| 5.3 | A status page or incident channel exists and users know where to look during an outage. | Communications lead | Status page URL and the launch announcement linking to it. | [ ] |
| 5.4 | User-facing copy explains fees, the dispute window, refunds, custody of the beneficiary key, and that the contracts are audited (or not). | Communications lead | Approved copy and legal/compliance sign-off. | [ ] |
| 5.5 | Launch and maintenance notices are drafted, including a template for "transactions are paused/under investigation". | Communications lead | Notice templates and the approval record. | [ ] |
| 5.6 | Providers, attesters and funder support know the escalation path for a stuck voucher or a disputed payment. | Operations lead | Support runbook and a walkthrough with the support owner. | [ ] |

## 6. Rollback

The contracts have no pause or upgrade function. A rollback therefore means
containing the incident, protecting escrow, and moving users to a reviewed
replacement deployment. Rehearse the steps before launch.

| # | Requirement | Owner | Evidence needed | Done |
| --- | --- | --- | --- | --- |
| 6.1 | A written rollback/containment runbook exists for each failure mode: bad token, bad parameter, compromised admin signer, indexer outage, and frontend/API outage. | Incident commander | Runbook link and review sign-off. | [ ] |
| 6.2 | The team knows which actions are possible on chain: suspend providers, propose/accept a new admin, repoint the receipt minter, and deploy reviewed replacement contracts. There is no way to reverse a settled voucher. | Contracts maintainer | A one-page control matrix and a tabletop walkthrough of two scenarios. | [ ] |
| 6.3 | A replacement-deployment procedure exists that preserves the old addresses for audit and migrates users without reusing the old `deployments/mainnet.env`. | Release manager | Procedure document and a testnet rehearsal with recorded contract ids. | [ ] |
| 6.4 | The frontend and indexer can be rolled back independently, and the last known-good builds/configuration are recorded. | Operations lead | Deployment history and the last known-good version identifiers. | [ ] |
| 6.5 | A communications template for a rollback, including user impact and recovery steps, is ready. | Communications lead | Template link and approval. | [ ] |
| 6.6 | The rollback rehearsal was run on testnet end to end, including a provider suspension and an admin handover. | Incident commander | Rehearsal date and transaction hashes. | [ ] |

## Final sign-off

| Decision | Owner | Evidence | Date |
| --- | --- | --- | --- |
| Audit accepted | Security reviewer | Audit report and finding closure record | |
| Code and release frozen | Contracts maintainer | Commit SHA, tag and `SHA256SUMS` | |
| Keys and custody verified | Operations lead | Multisig/Horizon evidence and custody attestation | |
| Parameters approved | Release manager | `get_config`, `quote` and approvals | |
| Monitoring live | Operations lead | Dashboard, alert and readiness evidence | |
| Communications ready | Communications lead | Contact sheet, status page and notices | |
| Rollback rehearsed | Incident commander | Runbook and rehearsal transaction hashes | |
| **Launch approved** | Incident commander | This checklist complete and signed | |

## What breaks if this is wrong

Skipping a gate does not produce a warning at runtime. It produces a live
contract with the wrong token, a single key that can resolve every dispute, an
unmonitored escrow balance, or an incident with no one to call. Contract
mistakes on mainnet are permanent.
