---
title: "0000 — Template"
parent: Architecture decision records
nav_order: 0
nav_exclude: true
---

# 0000 — Short title in the present tense

- **Status:** Proposed | Accepted | Superseded by `NNNN-short-title.md` (link it)
- **Date:** YYYY-MM-DD
- **Decision area:** contracts / docs / operations

## Context

What made this a decision rather than a default. Name the constraint, the
failure it prevents, or the two options that pull in different directions.
State the facts a reviewer cannot get from the code: who is trusted, what is
off chain, what breaks if the wrong choice is made. Keep it to a paragraph or
two.

## Decision

One or two sentences, in the active voice: "The contract rejects …", "The
reference is …". Then the detail that a reviewer needs to check the code
against — the exact check, the exact ordering, the exact bound. Link the
function that implements it:

```text
contracts/<crate>/src/lib.rs — `CrateName::function_name`
```

## Consequences

What follows, including the costs we accepted. Cover funds and privacy first,
then operability: who has to act, what can no longer be done, what a client or
indexer has to know, what the failure mode looks like. Be honest about the
limits — an ADR that claims more than the code does is worse than none.

## Alternatives rejected

One bullet per alternative, each with the reason it lost. "Rejected because it
is more work" is not a reason; say what it would have cost in funds, privacy,
or trust. Include the superficially simpler option someone will propose again.
