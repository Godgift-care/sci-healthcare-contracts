---
title: Incident response
nav_order: 15
---

# Incident response runbook

The procedure when a bug is found in a contract, a key is compromised, or a clinic or attester is acting in bad faith. Agree on it before you need it: in an incident, a checklist that everyone has already read is the difference between a contained problem and lost funds.

Every step names the exact function or command. Commands assume the shell setup below, and the roles named in the [mainnet launch checklist](mainnet-checklist.md#owners): the **incident commander** decides, the **operations lead** runs commands, the **communications lead** talks to users.

```bash
# Contract ids for the affected network, written by scripts/deploy.sh.
set -a; source deployments/testnet.env; set +a
NET=testnet
ADMIN=sci-admin                          # or the multisig account; see multisig-admin.md
ADMIN_ADDR="$(stellar keys address "$ADMIN")"
```

Read-only calls take `--send=no` and need no admin signature. Calls that change state are signed by `$ADMIN`; with a multisig admin, build them with `--build-only` and collect the signatures as in [Using a multisig admin](multisig-admin.md#7-make-an-admin-call-with-the-multisig).

## What you cannot do — know this first

- **There is no contract-wide pause.** An emergency pause that can never trap funds is tracked in [#13](https://github.com/Godgift-care/sci-healthcare-contracts/issues/13) and is not built. Until it is, containment is per clinic, per attester and per service, below.
- **Settled money cannot be recalled.** A voucher whose dispute window has closed can be settled by anyone, and settlement is final.
- **The admin cannot move escrow anywhere but the two parties.** `resolve_dispute` can only refund the funder or pay the clinic. That limits what an attacker with the admin key can do — and what you can do in response.
- **The contracts cannot be upgraded in place.** None has an upgrade entry point. A fix ships as a new deployment; see [Recovery](#5-recovery).

## 1. Triage (first 30 minutes)

1. **Open an incident.** The incident commander starts a private channel and a log: what was reported, by whom, when (UTC). Every command below goes into the log with its output.
2. **Classify it:**

   | Class | Examples | Go to |
   | --- | --- | --- |
   | A. Compromised admin key | admin calls nobody made, a `propose_admin` nobody intended | [Admin rotation](#3-admin-rotation) first, then containment |
   | B. Compromised attester key | attestations an attester says they did not make | [Containment](#2-containment): remove the attester |
   | C. Fraudulent clinic | claims for care that did not happen | [Containment](#2-containment): suspend the clinic |
   | D. Contract bug | funds moving in a way the code should not allow | [Containment](#2-containment) for what is exposed, then [Recovery](#5-recovery) |

3. **Measure what is exposed.** Find the vouchers the incident can still reach — `Claimed` and `Attested` ones whose dispute window has not closed:

   ```bash
   stellar contract invoke --id "$VOUCHER_CONTRACT_ID" --source-account "$ADMIN" --network "$NET" \
     --send=no -- next_voucher_id
   stellar contract invoke --id "$VOUCHER_CONTRACT_ID" --source-account "$ADMIN" --network "$NET" \
     --send=no -- get_voucher --voucher_id <id>
   ```

   The indexer in `sci-healthcare-backend` lists them by clinic and attester faster; use the chain to confirm. Note each voucher's `status` and `dispute_deadline`: every `Attested` voucher settles irreversibly once `dispute_deadline` passes.

4. **Check the admin is still yours** before relying on it:

   ```bash
   stellar contract invoke --id "$REGISTRY_CONTRACT_ID" --source-account "$ADMIN" --network "$NET" --send=no -- get_admin
   stellar contract invoke --id "$VOUCHER_CONTRACT_ID"  --source-account "$ADMIN" --network "$NET" --send=no -- get_config
   stellar contract invoke --id "$RECEIPT_CONTRACT_ID"  --source-account "$ADMIN" --network "$NET" --send=no -- get_admin
   stellar contract invoke --id "$REGISTRY_CONTRACT_ID" --source-account "$ADMIN" --network "$NET" --send=no -- get_pending_admin
   ```

   A `get_pending_admin` you did not propose means the admin key is in someone else's hands: go to [Admin rotation](#3-admin-rotation) now.

## 2. Containment

Use the narrowest lever that stops the harm. Each is one call and can be undone.

**Suspend a clinic** — stops new vouchers to it and stops it claiming. Vouchers already attested still settle, so pair it with disputes (below).

```bash
# ProviderStatus: 0 Pending, 1 Active, 2 Suspended
stellar contract invoke --id "$REGISTRY_CONTRACT_ID" --source-account "$ADMIN" --network "$NET" \
  -- set_provider_status --admin "$ADMIN_ADDR" --provider_addr <clinic G...> --status 2
```

**Remove an attester** — every further attestation by that key is rejected.

```bash
stellar contract invoke --id "$REGISTRY_CONTRACT_ID" --source-account "$ADMIN" --network "$NET" \
  -- remove_attester --admin "$ADMIN_ADDR" --attester <attester G...>
```

**Slow every attester down** — while you work out which attester is compromised, cut the daily cap. It takes effect at once, including for today's count, and must be at least 1.

```bash
stellar contract invoke --id "$VOUCHER_CONTRACT_ID" --source-account "$ADMIN" --network "$NET" \
  -- set_attestation_cap --admin "$ADMIN_ADDR" --cap 1
```

**Stop funds leaving on a bad attestation** — only the **funder** can dispute (`dispute(funder, voucher_id, reason_code)`), and only before `dispute_deadline`. Contact the funders of every exposed `Attested` voucher now, through the app's notification channel, and ask them to dispute. A disputed voucher is frozen until the admin resolves it.

**Resolve disputes** once the facts are known — `refund_funder: true` returns the full amount to the funder, `false` pays the clinic:

```bash
stellar contract invoke --id "$VOUCHER_CONTRACT_ID" --source-account "$ADMIN" --network "$NET" \
  -- resolve_dispute --admin "$ADMIN_ADDR" --voucher_id <id> --refund_funder true
```

**A clinic can pause a service itself** (out of stock, staff problem) with `set_service_available(provider_addr, code, false)`, signed by the clinic. New vouchers for that service are refused; ask the clinic to do it when the problem is limited to one service.

**Testnet faucet** — stop drips without giving up the token admin:

```bash
stellar contract invoke --id "$FAUCET_CONTRACT_ID" --source-account "$ADMIN" --network "$NET" \
  -- set_paused --admin "$ADMIN_ADDR" --paused true
```

## 3. Admin rotation

When the admin key is or may be compromised, move every contract to a new key — on mainnet, a new multisig account. Rotation is two steps per contract, so a typo cannot strand the protocol: nothing changes until the new key accepts.

```bash
NEW=sci-admin-2                          # the new key, already generated and funded
NEW_ADDR="$(stellar keys address "$NEW")"

for id in "$REGISTRY_CONTRACT_ID" "$VOUCHER_CONTRACT_ID" "$RECEIPT_CONTRACT_ID"; do
  stellar contract invoke --id "$id" --source-account "$ADMIN" --network "$NET" \
    -- propose_admin --admin "$ADMIN_ADDR" --new_admin "$NEW_ADDR"
done
for id in "$REGISTRY_CONTRACT_ID" "$VOUCHER_CONTRACT_ID" "$RECEIPT_CONTRACT_ID"; do
  stellar contract invoke --id "$id" --source-account "$NEW" --network "$NET" \
    -- accept_admin --new_admin "$NEW_ADDR"
done
```

Then check every contract reports the new admin with the read-only calls in [Triage](#1-triage-first-30-minutes) step 4, and update `ADMIN_ADDRESS` in `deployments/<network>.env`.

**If an attacker has already proposed their own address**, propose yours: proposing again replaces the nominee, and only the nominee can accept. Race them — this is why the old key must still be usable, and why mainnet admin should be a multisig whose signers cannot all be taken at once. If the attacker has *already accepted*, the contract is theirs: go straight to [Recovery](#5-recovery) and [Communication](#4-communication). Their reach is still bounded by the contract: they can suspend clinics and route disputed funds to one of the two parties, but they cannot send escrow to themselves.

## 4. Communication

The communications lead owns this; the incident commander approves every message.

1. **Within 1 hour of confirming an incident:** a short notice on the status page and in the app — what is affected (for example "new vouchers to some clinics are paused"), what is not, and what users should do. Funders of exposed vouchers are contacted directly and asked to dispute before their deadline.
2. **Never** publish details that let someone exploit an unfixed bug. Say what is affected, not how.
3. **Every 4 hours** until resolved: what changed since the last update.
4. **Security reporters** are answered as `SECURITY.md` commits to: acknowledgement within 72 hours, assessment within 7 days.
5. **When resolved:** a closing notice with the outcome and the date the post-mortem will be published.

## 5. Recovery

- **Contained, no bug:** restore what you changed — `set_provider_status ... --status 1` for a cleared clinic, `add_attester` for a cleared attester, `set_attestation_cap` back to its previous value (the default is 100), `set_paused ... --paused false` on the faucet.
- **Contract bug:** fix it, test it, release it, and deploy a new set with `scripts/deploy.sh <network> --force`, then `scripts/verify-deployments.sh <network>`. Vouchers in the old contracts are not migrated: they still settle and refund there, so keep the old contract ids in the app until every old voucher is settled or refunded. Point new funding at the new contracts only.
- **Admin lost to an attacker:** the same — redeploy, move clinics and attesters to the new registry, and tell funders and clinics which contracts to trust.

## 6. Post-mortem

Within 5 working days, publish a blameless post-mortem in the repository's security advisories (or `docs/incidents/` once public):

```markdown
# Incident <date>: <one-line summary>

- Detected: <UTC time, by whom, how>
- Contained: <UTC time, which levers>
- Resolved: <UTC time>
- Impact: <vouchers, amounts, users affected; funds lost if any>

## What happened
## Timeline (UTC)
## What went well
## What went badly
## Action items  (owner, issue link, due date)
```

Each action item gets an issue. Add anything this runbook got wrong to it in the same change.

## Rehearse it

Run this runbook against a local node or testnet before mainnet, and after every change to an admin function: deploy with `scripts/deploy.sh`, seed with `scripts/seed.sh`, then walk through [Triage](#1-triage-first-30-minutes) step 4, every [Containment](#2-containment) lever and its undo, and an [Admin rotation](#3-admin-rotation) to a throwaway key. Log how long each took.
