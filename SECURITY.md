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

You can expect an acknowledgement within **72 hours** and an assessment within
**7 days**. We will tell you when a fix has shipped and credit you in the release
notes unless you would rather stay anonymous.

## Scope

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
