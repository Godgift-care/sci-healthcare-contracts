---
title: Running a settlement keeper
nav_order: 11
---

# Running a settlement or matching keeper

A voucher only moves when somebody sends a transaction. Most steps belong to
one party: the funder funds, the clinic claims, the attester attests. The last
step does not. Once a voucher is due, **anyone** can call `settle` or `refund`,
and the money goes to the clinic or back to the funder. The caller gets
nothing and pays the fee.

A **keeper** is a small bot that makes those calls on everyone's behalf. Clinics
are paid as soon as the dispute window closes, not when someone remembers to
press settle, and funders get expired vouchers back without watching the
calendar. The protocol does not depend on a keeper existing
([ADR 0002](../adr/0002-settle-and-refund-are-permissionless.md)); a keeper
just makes things faster. Anyone can run one, and several can run at once.

This page lists what a keeper may call, why each call is safe to leave open,
how to find due vouchers, what it costs, and a minimal loop. The repository's
own keeper, [`examples/keeper.ts`](../../examples/keeper.ts), is a complete
implementation of this page.

## The permissionless functions

The functions below change state and require **no signature**. They are the
rows marked `none (permissionless)` in the
[authorisation matrices](contracts.md#authorisation-matrix), which CI checks
against the code. If a `require_auth` were added to one of them, the matrix
test would fail.

| Contract | Function | Due when | Money goes to | Why it is safe for anyone to call |
| --- | --- | --- | --- | --- |
| `voucher` | `settle(voucher_id)` | Status is `Attested` and `now ≥ dispute_deadline`. | Clinic (`amount - fee`), attester (its share), fee account (the rest), all as stored on the voucher. | Arguments are only the voucher id. Recipients and amounts come from storage, and timing comes from the ledger clock, so a caller can only make a payout happen at the moment it is already owed. Calling too early fails with `DisputeWindowOpen`. Calling twice fails with `InvalidStatus`, so a voucher cannot be paid twice. |
| `voucher` | `refund(voucher_id)` | Status is `Funded` and either `now ≥ expires_at` or the clinic is no longer active, **or** status is `Claimed` and `now ≥ expires_at + 7 days`. `refundable_at(voucher_id)` returns the exact time. | The stored funder, in full, with no fee. | Same reasoning: no recipient, amount or timing can be chosen. Calling too early fails with `NotYetExpired`. A voucher that is `Attested`, `Disputed`, `Settled` or `Refunded` fails with `InvalidStatus`. A claimed voucher keeps its 7-day grace even if the clinic is suspended, because care may already have happened. |
| `sponsor` | `reconcile_sponsor(sponsor)` | Any time. It is useful after a matched voucher settles or refunds. | No token moves. It updates the sponsor desk's books. | It only reads the matching vouchers' final status from the voucher contract and records it. A settled match stays spent, and a refunded match returns to the sponsor's available budget. It cannot move money, and calling it again changes nothing. `match_voucher` and `withdraw` call it themselves, so a keeper only makes the budget accurate sooner. |

`initialize` also takes no signature, but it runs only once, at deployment. It
is not a keeper call.

Everything else that changes state needs a specific signer: the funder,
clinic, attester, sponsor or admin. In particular, **matching** a voucher
(`sponsor.match_voucher`) is signed by the sponsor whose budget pays for the
match. A third-party bot cannot match on someone else's behalf. A "matching
bot" is a sponsor's own automation holding the sponsor's key, or a bot that
only calls `reconcile_sponsor`.

### What can go wrong for the caller

Each of these errors reverts the whole transaction. The voucher keeps its
status, no receipt is minted and nothing is half-paid. The keeper loses only
the fee of the failed transaction. Simulating before submitting (see below)
means the keeper does not pay for most of these.

| Error | Meaning | What the keeper should do |
| --- | --- | --- |
| `InvalidStatus` (#10) | Someone else got there first, or the voucher was disputed. | Drop it. |
| `DisputeWindowOpen` (#13) | `settle` before `dispute_deadline`. | Retry after the deadline. |
| `NotYetExpired` (#12) | `refund` before `refundable_at`. | Retry after `refundable_at`. |
| `EscrowShortfall` (#23) | The token issuer clawed back escrow. | Stop retrying and report it. See [the settlement token's issuer](../protocol/trust.md#the-settlement-tokens-issuer). |
| `TokenTransferFailed` (#24) | The issuer froze the escrow or the recipient. | Retry later; it settles once the freeze is lifted. |
| `TokenAmountMismatch` (#34) | The token moved a different amount than requested. Only a non-standard token does this. | Stop and report it. It should never happen with USDC. |

## Finding due vouchers

A keeper needs a list of vouchers that are, or soon will be, due. It can get
one in three ways, cheapest first:

1. **The backend API.** `GET /vouchers?status=attested` and
   `GET /vouchers?status=funded` / `claimed`
   ([API reference](api.md#get-vouchers)) return vouchers with their deadlines.
   This is what `examples/keeper.ts` does. Treat the API as a hint, not the
   truth: always re-check the chain before sending.
2. **Events.** Without the backend, follow the voucher contract's events
   ([Reading contract events](events.md)). `voucher_attested` carries
   `dispute_deadline`, so the voucher is settleable from that time.
   `voucher_created` carries `expires_at`, so it is refundable from then if
   nobody claims it. `voucher_claimed` makes it refundable at
   `expires_at + 7 days` if nobody attests it. `voucher_settled`,
   `voucher_refunded` and `voucher_disputed` take it off the list.
3. **Scanning the chain.** `next_voucher_id()` is one past the newest id.
   `statuses(ids)` returns the status of up to `MAX_STATUSES_PER_CALL` (50)
   vouchers per call as a read-only view, without bumping their TTL. For each
   `Attested` id, `get_voucher(id).dispute_deadline` gives the settle time.
   For each `Funded` or `Claimed` id, `refundable_at(id)` gives the refund
   time. A scan is slow for a large deployment, so use it to backfill, not
   every pass.

Before sending, **re-check on chain** with `get_voucher` and `refundable_at`,
and **simulate** the exact transaction. If the simulation fails, skip the
voucher this pass. Most often another keeper settled it first, or a funder
disputed it.

## What it costs

The keeper account needs **XLM only**. It needs no USDC trustline, because
`settle` and `refund` never pay the caller.

- **Transaction fee.** The base fee (100 stroops per operation) plus the
  resource fee for CPU, ledger reads and writes, and event bytes. The resource
  fee comes back from `simulateTransaction` as `minResourceFee`. Use that
  number instead of a guess, because it changes with network settings.
- **Rent, on `settle` only.** A settlement mints a care receipt. The receipt
  book creates `Receipt(voucher_id)` and the provider-index entries, plus
  `Count(beneficiary_ref)` on a beneficiary's first receipt. **The account
  that submits `settle` pays their initial rent**, which is up to about
  **1.17 XLM per settlement** at the conservative parameters in
  [Storage and rent costs](../protocol/costs.md#what-a-voucher-costs). This
  is the main running cost of a settlement keeper. Budget for it, or arrange
  for clinics to settle their own vouchers when keeper funds are short.
- **Refunds** create no new entries, so a refund costs only the transaction
  fee.
- **`reconcile_sponsor`** is a transaction fee only.
- **Lost races.** Two keepers can submit the same voucher in the same ledger.
  One succeeds. The other fails with `InvalidStatus` and pays its fee. This
  costs a little money but is otherwise harmless. To make it rarer, add a
  random delay or split vouchers between keepers by id.

## A minimal loop

The loop below is the shape of `examples/keeper.ts`, without the error
handling and configuration:

```ts
// VoucherStatus discriminants, as get_voucher returns them.
const FUNDED = 0, CLAIMED = 1, ATTESTED = 2;

// Every pass: find candidates, verify on chain, simulate, then send.
for (;;) {
  const now = BigInt(Math.floor(Date.now() / 1000));

  for (const v of await dueFromApi()) {          // hint only
    const onChain = await read('get_voucher', [u64(v.id)]);

    let call: 'settle' | 'refund' | null = null;
    const status = Number(onChain.status);
    if (status === ATTESTED && now >= BigInt(onChain.dispute_deadline)) {
      call = 'settle';
    } else if (
      (status === FUNDED || status === CLAIMED) &&
      now >= BigInt(await read('refundable_at', [u64(v.id)]))
    ) {
      call = 'refund';
    }
    if (!call) continue;                          // not due, or already final

    const tx = await build(call, [u64(v.id)]);    // signed by the keeper key
    const sim = await server.simulateTransaction(tx);
    if (rpc.Api.isSimulationError(sim)) continue; // someone beat us, or a dispute
    await submit(rpc.assembleTransaction(tx, sim).build());
  }

  await sleep(30_000);
}
```

To try it on testnet without signing anything:

```bash
cd examples
npm install
npm run keeper -- --dry-run          # print what it would do
KEEPER_SECRET=<funded account seed> npm run keeper -- --watch
```

## Operating notes

- **Use a dedicated key.** The keeper key can only spend its own XLM, but keep
  it apart from any admin, funder or clinic key anyway, so a leak costs no
  more than the float.
- **Watch the float.** Alert when the keeper's XLM balance falls below a few
  days of settlements, including receipt rent.
- **Do not retry every error forever.** `EscrowShortfall` and
  `TokenAmountMismatch` mean something is wrong with the token. Page a human
  ([incident response](incident-response.md)) instead of spending fees on
  the same voucher every pass.
- **Being late is safe.** If the keeper stops, nothing is lost. Settlement
  and refund stay available to the clinic, the funder or any other keeper.
