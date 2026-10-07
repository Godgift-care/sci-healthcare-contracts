---
title: Funding care for someone
nav_order: 6
---

# Funding care for someone

You are paying for a specific treatment at a specific clinic, for yourself or for someone else.

## What you need

- A Stellar wallet. [Freighter](https://freighter.app) is the usual choice.
- Your wallet set to **Testnet** while the protocol is in testing.
- Some USDC in that wallet, and a trustline for it (see below).
- The patient's clinic reference and a secret key you both agree on.

## About the trustline

Stellar requires you to explicitly opt in to holding an asset before you can receive it. This is called a trustline, and without one for USDC your wallet cannot hold it and funding will fail.

Most wallets offer to add one when you first try to receive an asset. If yours does not, the app will prompt you.

## Funding

1. **Connect your wallet.** Top right. Nothing is stored; you can disconnect any time.
2. **Pick the clinic.** The directory shows only verified clinics, with their listed prices.
3. **Pick the service.** Prices are set by the clinic and published on chain.
4. **Enter the patient reference and secret key.**

   The reference is whatever identifies the patient at that clinic — a card number, a registration number. The key is a shared secret of at least 32 characters.

   These two are combined *in your browser* into an opaque value. Neither leaves your device. Neither is sent to us. Neither goes on the blockchain.

   **Use the same reference and key every time for the same person.** They are what links their care history together. If you lose the key, past receipts cannot be looked up again. There is no recovery — that is the cost of us not holding it.

5. **Confirm the amount.** You can pay more than the listed price, never less.
6. **Sign.** Your wallet shows the transaction. Approving moves the USDC into the contract.

Your money is now escrowed. It is not the clinic's yet.

## After funding

Watch it under **My vouchers**.

| Status | What it means |
| --- | --- |
| Funded | Escrowed. The patient has not attended yet. |
| Claimed | The clinic says the patient turned up. |
| Attested | An attester confirmed delivery. Dispute window is open. |
| Settled | The clinic has been paid. |
| Refunded | The money came back to you. |

## If something goes wrong

**The patient never went.** Wait for the expiry date, then press **Refund me**. You get the full amount back, no fee. Anyone can trigger this, so you are not dependent on us.

**The clinic claimed, but nobody ever confirmed the visit.** You can dispute straight away. If you do nothing, the voucher becomes refundable seven days after its expiry date and **Refund me** appears again.

**The clinic claimed but did not treat them.** Press **Dispute** while the status is `Claimed` or `Attested`. Funds freeze and an administrator reviews it. Do this before the dispute window closes — 72 hours after attestation. Once it closes, settlement is automatic and irreversible.

**The window closed and you missed it.** The money has gone to the clinic. The protocol cannot claw it back. Raise it with the clinic directly.

## What this does not do

It does not guarantee the care was *good*. It guarantees an independent party attested it happened, and that you had a window to object.
