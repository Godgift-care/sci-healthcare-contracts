---
title: Architecture decision records
nav_order: 16
has_children: true
permalink: /adr/
---

# Architecture decision records

An ADR records one decision that is not obvious from reading the code: what was
in front of us, what we chose, what follows from it, and what we rejected. The
point is that a reviewer meets the reasoning where they are already looking,
instead of finding it in a comment deep inside a contract that someone later
deletes as noise.

These are the decisions that protect funds or privacy. If a change would undo
one, the ADR is the argument you have to answer.

| ADR | Decision | Code |
| --- | --- | --- |
| [0001](0001-provider-cannot-attest-its-own-voucher.md) | A provider cannot attest its own voucher | `voucher::attest` |
| [0002](0002-settle-and-refund-are-permissionless.md) | `settle` and `refund` need no signature | `voucher::settle`, `voucher::refund` |
| [0003](0003-beneficiary-references-are-hmacs.md) | Beneficiary references are HMACs, not hashes | `Voucher::beneficiary_ref` |
| [0004](0004-no-general-balance.md) | The contract holds no general balance | `voucher::fund`, `voucher::escrow_summary` |

New ADRs copy [0000-template.md](0000-template.md) and take the next number.
An ADR is never rewritten to match a later decision: write a new one that
supersedes it and says so at the top, so the history of the reasoning survives.

A note on sourcing: every factual claim below points at the function that
implements it. Where a decision is enforced outside the contracts (the HMAC
derivation in [ADR 0003](0003-beneficiary-references-are-hmacs.md)), the ADR
says so plainly rather than implying the chain checks it.
