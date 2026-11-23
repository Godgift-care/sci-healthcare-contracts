---
title: Error code reference
nav_order: 10
---

# Error code reference

Every contract returns its failures as a numbered contract error. A failed
simulation or transaction reports it as `Error(Contract, #N)`; the number alone
does not say which contract raised it, so read it together with the contract
you called (or, for a cross-contract failure, the contract named in the
diagnostic events).

Codes are stable: a variant is never renumbered, and new errors take the next
free code. This page is checked against the source by
`contracts/common/tests/error_docs.rs`, which fails when a variant of any
contract's error enum is missing here or listed with the wrong code.

Errors that are not in these tables come from somewhere else: the settlement
token (for example `Error(Contract, #13)` from a Stellar asset contract when a
trustline is missing), or the host itself (`Error(Auth, ...)` when a required
signature is absent, `Error(Budget, ...)` when a call runs out of resources).

## registry

Source: `RegistryError` in `contracts/registry/src/types.rs`.

| Code | Name | Meaning | What to do |
| --- | --- | --- | --- |
| 1 | `AlreadyInitialized` | `initialize` was called on a contract that is already set up. | Nothing; use the existing deployment. |
| 2 | `NotInitialized` | The contract has not been initialised yet. | Call `initialize`, or check you are using the right contract id. |
| 3 | `NotAuthorized` | The caller is not the admin (or the provider) this function requires. | Sign with the admin or provider account the function names. |
| 4 | `ProviderNotFound` | No provider is registered at that address. | Check the address, or register the provider first. |
| 5 | `ProviderExists` | A provider is already registered at that address. | Update the existing provider instead of registering again. |
| 6 | `ProviderNotActive` | The provider is pending, suspended or retired. | Wait for the admin to activate or reinstate the clinic. |
| 7 | `ServiceNotFound` | The provider does not list that service code. | Check the code with `get_service`, or `upsert_service` it first. |
| 8 | `InvalidPrice` | The price is zero or negative. | Pass a positive price in the token's smallest unit. |
| 9 | `InvalidCountry` | The country is not a two-letter ISO 3166-1 alpha-2 code. | Pass an upper-case code such as `NG` or `KE`. |
| 10 | `EmptyName` | The provider or service name is empty. | Pass a non-empty name. |
| 11 | `NoPendingAdmin` | `accept_admin` was called with no nomination waiting. | Have the current admin call `propose_admin` first. |
| 12 | `PriceNoticeTooShort` | A scheduled price takes effect sooner than `MIN_PRICE_NOTICE_SECS`. | Choose a later `effective_at`. |
| 13 | `PriceIncreaseNeedsNotice` | `upsert_service` tried to raise a listed price at once. | Raise prices through `schedule_service_price`. |
| 14 | `InvalidTransition` | The status change is not an allowed transition, or would leave `Retired`. | Check the allowed transitions in the contract reference. |
| 15 | `LargePriceJumpNeedsConfirmation` | The scheduled price is `MAX_PRICE_JUMP` times the current one or more. | Fix the typo, or pass `confirm_large_change = true` if it is intended. |
| 16 | `MissingReasonCode` | `set_provider_status` was called with `reason_code` 0. | Pass a non-zero reason code from the trust model. |

## voucher

Source: `VoucherError` in `contracts/voucher/src/types.rs`.

| Code | Name | Meaning | What to do |
| --- | --- | --- | --- |
| 1 | `AlreadyInitialized` | `initialize` was called on a contract that is already set up. | Nothing; use the existing deployment. |
| 2 | `NotInitialized` | The contract has not been initialised yet. | Call `initialize`, or check the contract id. |
| 3 | `NotAuthorized` | The caller is not the party this function requires (admin, funder, provider or attester). | Sign with the account the function names; a provider cannot attest its own voucher. |
| 4 | `VoucherNotFound` | No voucher has that id. | Check the id returned by `create_voucher`. |
| 5 | `ProviderNotActive` | The clinic is not active in the registry. | Choose an active clinic, or wait until it is reinstated. |
| 6 | `ServiceNotOffered` | The clinic does not list that service code. | Pick a service from the clinic's registry listing. |
| 7 | `AmountBelowPrice` | The amount is below the service's registry price. | Fund at least the listed price. |
| 8 | `InvalidAmount` | The amount is zero or negative. | Pass a positive amount. |
| 9 | `InvalidExpiry` | The expiry is in the past or outside the allowed range. | Choose an expiry within the allowed range. |
| 10 | `InvalidStatus` | The voucher is not in the status this call needs. | Read the voucher's status and follow the lifecycle. |
| 11 | `VoucherExpired` | The voucher's expiry has passed. | Refund it instead. |
| 12 | `NotYetExpired` | A refund was attempted before the voucher is refundable. | Wait until `refundable_at`. |
| 13 | `DisputeWindowOpen` | Settlement was attempted while the dispute window is still open. | Wait until the dispute deadline has passed. |
| 14 | `DisputeWindowClosed` | A dispute was raised after the window closed. | The voucher can only settle now. |
| 15 | `InvalidFee` | The fee is above the allowed maximum. | Pass a fee within the documented bound. |
| 16 | `MathOverflow` | A counter or amount would overflow. | Use a smaller amount; report it if it happens with normal values. |
| 17 | `NoPendingAdmin` | `accept_admin` was called with no nomination waiting. | Have the current admin call `propose_admin` first. |
| 18 | `ServiceUnavailable` | The clinic has paused this service. | Wait until it is made available again, or pick another service. |
| 19 | `AttestationCapReached` | The attester has used up today's attestations. | Try again tomorrow (UTC) or ask the admin to raise the cap. |
| 20 | `InvalidAttestationCap` | The attestation cap is below 1. | Pass a cap of at least 1. |
| 21 | `InvalidAttesterShare` | `attester_share_bps` is greater than `fee_bps`. | Keep the attester share within the protocol fee. |
| 22 | `AmountNotReduced` | The new amount is not lower than the amount escrowed. | Pass a lower amount, or leave the voucher as it is. |
| 23 | `EscrowShortfall` | The contract holds less of the token than this voucher escrows (issuer clawback). | Contact the admin; see the trust model on the token's issuer. |
| 24 | `TokenTransferFailed` | The token refused the payment, typically because the issuer froze an account. | Nothing moved; retry once the freeze is lifted. |
| 25 | `ProviderAtCapacity` | The clinic already has as many outstanding vouchers as its declared capacity. | Wait for vouchers to settle, or choose another clinic. |
| 26 | `EscrowCapReached` | Funding would take the clinic's escrow above the per-clinic cap. | Fund a smaller amount later, or choose another clinic. |
| 27 | `InvalidEscrowCap` | The per-clinic escrow cap is negative. | Pass zero or a positive cap. |
| 28 | `PresenceProofRequired` | The voucher has a presence commitment, so it must be claimed with proof. | Call `claim_with_proof` with the patient's secret. |
| 29 | `InvalidPresenceProof` | The secret does not hash to the presence commitment. | Ask the patient for the correct secret. |
| 30 | `NoPresenceCommitment` | `claim_with_proof` on a voucher funded without a commitment. | Call `claim` instead. |
| 31 | `InvalidDisputeWindow` | The dispute window, or the bounds themselves, are out of range. | Choose a window between the admin-set minimum and maximum. |
| 32 | `PresenceCommitmentRequired` | The voucher is above the presence threshold but has no commitment. | Fund it with a `presence_commitment`. |
| 33 | `InvalidPresenceThreshold` | The presence threshold is negative. | Pass zero or a positive threshold. |
| 34 | `TokenAmountMismatch` | The token moved a different amount than requested (fees or hooks). | Use a standard token such as USDC; the whole call reverted. |
| 42 | `PriceChanged` | The registry price changed between the quote and this call. | Re-quote and sign again; nothing was escrowed. |

## receipt

Source: `ReceiptError` in `contracts/receipt/src/lib.rs`.

| Code | Name | Meaning | What to do |
| --- | --- | --- | --- |
| 1 | `AlreadyInitialized` | `initialize` was called on a contract that is already set up. | Nothing; use the existing deployment. |
| 2 | `NotInitialized` | The contract has not been initialised yet. | Call `initialize`, or check the contract id. |
| 3 | `NotAuthorized` | The caller is not the admin or the minter. | Only the voucher contract mints; sign admin calls with the admin. |
| 4 | `ReceiptNotFound` | No receipt exists for that voucher id. | Check the id; receipts exist only for settled vouchers. |
| 5 | `ReceiptExists` | A receipt for that voucher was already minted. | Nothing; read the existing receipt. |
| 6 | `InvalidAmount` | The amount is zero or negative. | Pass a positive amount. |
| 7 | `NoPendingAdmin` | `accept_admin` was called with no nomination waiting. | Have the current admin call `propose_admin` first. |
| 8 | `NoPendingMinter` | `accept_minter` was called with no minter nomination waiting. | Have the admin call `propose_minter` first. |
| 9 | `MathOverflow` | A provider's receipt count or total would overflow. | The mint is refused; report it. |

## faucet (testnet only)

Source: `FaucetError` in `contracts/faucet/src/lib.rs`.

| Code | Name | Meaning | What to do |
| --- | --- | --- | --- |
| 1 | `AlreadyInitialized` | `initialize` was called on a contract that is already set up. | Nothing; use the existing deployment. |
| 2 | `NotInitialized` | The contract has not been initialised yet. | Call `initialize`, or check the contract id. |
| 3 | `NotAuthorized` | The caller is not the admin. | Sign with the faucet admin. |
| 4 | `InvalidAmount` | The drip amount or cooldown is invalid. | Pass a positive amount. |
| 5 | `CoolingDown` | This address dripped too recently. | Wait until `next_drip_at`. |
| 6 | `DailyCapReached` | Today's minting cap across all addresses is used up. | Try again tomorrow (UTC). |
| 7 | `Paused` | The faucet is paused. | Wait for the admin to unpause it. |
| 8 | `LifetimeCapReached` | This address has received its lifetime allowance. | Use another testnet account. |

## sponsor

Source: `SponsorError` in `contracts/sponsor/src/types.rs`.

| Code | Name | Meaning | What to do |
| --- | --- | --- | --- |
| 1 | `AlreadyInitialized` | `initialize` was called on a contract that is already set up. | Nothing; use the existing deployment. |
| 2 | `NotInitialized` | The contract has not been initialised yet. | Call `initialize`, or check the contract id. |
| 3 | `NotAuthorized` | The caller is not the sponsor or admin this function requires. | Sign with the account the function names. |
| 4 | `InvalidAmount` | The deposit or withdrawal is not positive. | Pass a positive amount. |
| 5 | `InvalidRules` | The rules are outside what the desk accepts. | Check `match_bps`, the cap and the lists against the limits. |
| 6 | `RulesNotSet` | The sponsor has not set any rules yet. | Call `set_rules` first. |
| 7 | `VoucherNotFound` | The voucher does not exist in the voucher contract. | Check the voucher id. |
| 8 | `VoucherNotFunded` | The voucher is not `Funded`. | Only funded vouchers can be matched. |
| 9 | `VoucherExpired` | The voucher has expired. | Pick a voucher that is still open. |
| 10 | `AlreadyMatched` | The voucher has already been matched. | Nothing; each voucher is matched once. |
| 11 | `SelfMatchNotAllowed` | The sponsor funded the voucher itself, or this contract did. | Match vouchers funded by someone else. |
| 12 | `CountryNotEligible` | The clinic is not in one of the sponsor's countries. | Pick an eligible voucher, or widen the rules. |
| 13 | `ServiceNotEligible` | The service is not one the sponsor matches. | Pick an eligible voucher, or widen the rules. |
| 14 | `VoucherTooNew` | The voucher was funded too recently to qualify. | Wait until `min_voucher_age_secs` has passed. |
| 15 | `VoucherTooSmall` | The voucher is below the sponsor's minimum. | Pick a larger voucher, or lower the minimum. |
| 16 | `MatchBelowPrice` | The match works out below the service price. | Raise the ratio or the per-voucher cap. |
| 17 | `BudgetExhausted` | The sponsor's budget cannot cover this match. | Deposit more budget. |
| 18 | `WithdrawExceedsBudget` | The withdrawal is larger than the unused budget. | Withdraw at most the available amount. |
| 19 | `TooManyOpenMatches` | The sponsor has as many open matches as the desk allows. | Wait for open matches to settle or refund. |
| 20 | `Paused` | The desk is paused. | Wait for the admin to unpause it. |
| 21 | `VoucherCallFailed` | The voucher contract refused the matching voucher. | Read the voucher contract's diagnostic event for its own error. |
| 22 | `ProviderNotEligible` | The clinic is not active in the registry. | Pick a voucher at an active clinic. |
| 23 | `ServiceUnavailable` | The clinic has paused this service. | Wait until the service is available again. |
| 24 | `MatchTooSmall` | The match works out to nothing. | Raise the ratio, or match a larger voucher. |
| 25 | `MathOverflow` | A counter would exceed what the token can represent. | Use smaller amounts; report it. |
| 26 | `NoPendingAdmin` | `accept_admin` was called with no nomination waiting. | Have the current admin call `propose_admin` first. |
