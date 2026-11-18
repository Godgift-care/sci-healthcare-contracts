# Security Policy

## Audit status

**These contracts and services are unaudited.** They have not been reviewed by a
third-party security firm. The current deployment is on Stellar **testnet** only
and holds no real funds. Do not deploy to mainnet or use with real value until an
audit has been completed and the
[mainnet launch checklist](docs/developers/mainnet-checklist.md) is signed off.

## Reporting a vulnerability

Please **do not open a public GitHub issue** for a security vulnerability.

Report privately to **adelekevat@gmail.com**, or through GitHub's
[private vulnerability reporting](https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing-information-about-vulnerabilities/privately-reporting-a-security-vulnerability)
on this repository.

Include:

- What the issue is and where in the code it lives
- Steps to reproduce, or a proof of concept
- What an attacker could achieve with it
- Any suggested fix
- How you would like to be credited, or whether you would rather stay anonymous

## What happens next

These are the response times the maintainers commit to, measured from the
report arriving:

| Stage | Commitment |
| --- | --- |
| Acknowledgement | Within **72 hours** |
| Assessment: severity, impact and a remediation plan | Within **7 days** |
| Fix and disclosure | We agree the timetable with you after the assessment, and tell you when a fix has shipped |

If a report is **critical**, say so in the subject line so it is triaged first.
The assessment may conclude that the report is out of scope or already known;
either way we will explain why.

## Severity levels

We use four levels. They describe impact, not how hard the bug is to find. The
response commitments above apply to every level.

| Severity | What it means | Examples |
| --- | --- | --- |
| **Critical** | Escrowed funds can be lost, frozen or stolen, or authorisation can be bypassed | A contract bug that pays an attacker, locks a funder's money permanently, or settles the same voucher twice |
| **High** | A core protocol promise can be broken without an admin key | Settling before the dispute window closes, settling without a valid attestation, refunding a settled voucher, the provider attesting its own payment |
| **Medium** | Limited loss, a way to block a specific user, or a misleading on-chain/API state | One voucher can be made unrefundable, a legitimate settlement can be blocked, the indexer records a status that contradicts the chain |
| **Low** | Hardening, defence in depth, or a leak with no direct fund impact | A missing check that needs an already-compromised key, metadata that reveals more than intended without identifying a patient, a docs error with security consequences |

Scope and trust assumptions matter more than the label. A report that depends on
the admin key being compromised, or on an attester colluding with a provider, is
out of scope even though the impact would be critical.

## Scope

This policy covers the Soroban contracts in this repository:

| Contract | Crate | Deployed on Stellar testnet |
| --- | --- | --- |
| Registry | `contracts/registry` | [`CAMU635NRFEHATJ6MTY5SI43IAZ43KHPFXAQDQAST2AHKCHLSGWHVE4V`](https://stellar.expert/explorer/testnet/contract/CAMU635NRFEHATJ6MTY5SI43IAZ43KHPFXAQDQAST2AHKCHLSGWHVE4V) |
| Voucher escrow | `contracts/voucher` | [`CBPUQN6CNJCNGPKNEXB7UM4T7RINKQB7HQI5CXEOSQ5EFRHE4G4VKPSQ`](https://stellar.expert/explorer/testnet/contract/CBPUQN6CNJCNGPKNEXB7UM4T7RINKQB7HQI5CXEOSQ5EFRHE4G4VKPSQ) |
| Care receipts | `contracts/receipt` | [`CBCJRI2BCZBMZNX77WWDC2NGSV4ZNAN5Q23XHYNBH6UTOQMPJHZGX5C7`](https://stellar.expert/explorer/testnet/contract/CBCJRI2BCZBMZNX77WWDC2NGSV4ZNAN5Q23XHYNBH6UTOQMPJHZGX5C7) |
| Test USDC faucet | `contracts/faucet` | [`CCB25WG7DQ2CSNXPTOHZO2PDZ324FQPZLH3LSNCCLWYM3FEH52GEXC47`](https://stellar.expert/explorer/testnet/contract/CCB25WG7DQ2CSNXPTOHZO2PDZ324FQPZLH3LSNCCLWYM3FEH52GEXC47) |
| Sponsorship desk | `contracts/sponsor` | Not deployed yet; the source is in scope |

The shared `contracts/common` crate is in scope through the contracts that use
it, and so are the deployment, seed and verification scripts under `scripts/`
and the TypeScript examples under `examples/` where a bug could move funds,
freeze funds or leak patient data. The only deployed network is **Stellar
testnet**; addresses are recorded in [`deployments/testnet.env`](deployments/testnet.env).
There is no mainnet deployment.

In scope:

- Loss, freezing, or theft of escrowed funds
- Bypassing authorisation on any contract function
- A provider settling a voucher without a valid attestation
- Attestation by the provider that stands to be paid
- Settling before the dispute window closes, or blocking a legitimate settlement
- Refund of an already-settled voucher, or double settlement
- Leakage of patient-identifying information through on-chain data or the API
- Indexer state corruption that misrepresents on-chain truth

Out of scope:

- Anything requiring the admin key to be compromised (the trust model documents
  admin powers explicitly)
- Attesters confirming care that did not happen — this is a known and documented
  trust assumption, not a vulnerability. Reports that *narrow* it are welcome.
- Denial of service against public Soroban RPC endpoints
- Missing rate limits on read-only endpoints already behind a rate limiter
- Testnet-only misconfiguration
- Bugs in third-party wallets, the Stellar network or the settlement token's
  issuer

## Recognition and rewards

**There is no monetary reward or bug bounty at this time.** We do not pay for
reports, and nothing in this policy creates an entitlement to payment. If that
changes, the funded programme and its rules will be published here; it will not
apply retroactively.

We do recognise good reports. Unless you ask to stay anonymous, we will credit
you by name or handle in the release notes and in the security advisory for the
issue, and we will link to your write-up if you publish one. Tell us in the
report how you want to be credited.

## Disclosure

Please keep the report private until a fix has been deployed or we have agreed a
disclosure date. We will not publish exploit details while a fix is outstanding.
When the fix ships we will publish a security advisory that describes the impact
and the remediation, and credits the reporter unless they prefer otherwise.

## If an incident is underway

The [incident response runbook](docs/developers/incident-response.md) is the
procedure for a live bug or a compromised key: triage, containment with the
exact contract calls, admin rotation, communication and the post-mortem.

## Known trust assumptions

These are design decisions, documented so they are not mistaken for bugs:

1. **Attesters are trusted** to confirm delivery honestly. The contract refuses
   attestation from the provider being paid, and the funder gets a dispute
   window, but a colluding attester and provider can still settle a voucher for
   care that did not happen.
2. **The admin resolves disputes.** The admin can route escrowed funds to either
   the funder or the provider, and can suspend providers. The admin cannot divert
   funds anywhere else.
3. **Beneficiary references are pseudonymous, not anonymous.** Anyone who already
   knows both a patient identifier and the HMAC key can confirm a match.
