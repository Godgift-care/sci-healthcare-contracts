---
title: Home
nav_order: 1
permalink: /
---

# What this is

SCI Healthcare is a way to pay for someone's medical care without handing over cash and hoping.

You fund a voucher for a **named clinic** and a **named service**. The money is held by a smart contract on Stellar. It is released to the clinic only after someone independent confirms the care actually happened. If it never happens, you get your money back.

The person paying does not have to be the patient. A nurse in London can fund her mother's antenatal visit in Lagos, and the money can only be spent on that visit.

## The problem this addresses

Out-of-pocket health spending pushes people into poverty at enormous scale. According to the WHO and World Bank [2025 Global Monitoring Report on Universal Health Coverage](https://www.worldbank.org/en/topic/universalhealthcoverage/publication/2025-global-monitoring-report-gmr):

- **1.4 billion people** incur catastrophic health expenditure
- **70 million people** are pushed into extreme poverty by it
- **435 million people** are pushed deeper into extreme poverty

Meanwhile, sub-Saharan Africa received an estimated **$56 billion in formal remittances in 2024**, and healthcare is among the primary uses of that money. But a remittance is just cash. Once it lands, there is no link between the money sent and the care it was sent for. It gets spent on the emergency in front of the household, which is often not the one the sender had in mind.

## What is actually different here

Three things, and none of them is "it uses a blockchain".

**The money is purpose-bound before it moves.** There is no wallet balance and no float. Funds enter the contract already attached to one clinic, one service and an expiry date. They can leave in exactly two directions: to that clinic once care is confirmed, or back to the funder. Nowhere else.

**The clinic cannot confirm its own payment.** The contract rejects an attestation from the provider that stands to receive the funds, even if that provider holds an attester role. Whoever gets paid and whoever confirms delivery are always different parties.

**Nobody has to be trusted to release the money.** Settlement and refund are permissionless. Once the dispute window closes, anyone can trigger payment to the clinic — the clinic does not depend on the funder, on us, or on any operator to get paid.

## Why Stellar

This design needs four things that are not decorative:

| Need | Why Stellar |
| --- | --- |
| Local currency at the end | Anchors already operate in the target corridors — Yellowcard across Africa, Cowrie in Nigeria, HoneyCoin in Kenya, MoneyGram for cash. This is the part no other chain has. |
| Vouchers worth $1–$5 | Fees are a fraction of a cent. A $3 consultation voucher is economically dead at $0.50 of gas. |
| Clinics with no crypto | Sponsored reserves let a rural clinic be onboarded holding zero XLM. |
| Programmable escrow | Soroban carries the lifecycle, the dispute window and the settlement logic. |

## Status

Deployed on **Stellar testnet**. Unaudited. No real funds.

| Contract | Address |
| --- | --- |
| Registry | [`CAMU635NRFEHATJ6MTY5SI43IAZ43KHPFXAQDQAST2AHKCHLSGWHVE4V`](https://stellar.expert/explorer/testnet/contract/CAMU635NRFEHATJ6MTY5SI43IAZ43KHPFXAQDQAST2AHKCHLSGWHVE4V) |
| Voucher escrow | [`CBPUQN6CNJCNGPKNEXB7UM4T7RINKQB7HQI5CXEOSQ5EFRHE4G4VKPSQ`](https://stellar.expert/explorer/testnet/contract/CBPUQN6CNJCNGPKNEXB7UM4T7RINKQB7HQI5CXEOSQ5EFRHE4G4VKPSQ) |
| Care receipts | [`CBCJRI2BCZBMZNX77WWDC2NGSV4ZNAN5Q23XHYNBH6UTOQMPJHZGX5C7`](https://stellar.expert/explorer/testnet/contract/CBCJRI2BCZBMZNX77WWDC2NGSV4ZNAN5Q23XHYNBH6UTOQMPJHZGX5C7) |
