use soroban_sdk::{contracterror, contracttype, Address, BytesN};

/// Lifecycle of a single care voucher.
///
/// ```text
///                  claim            attest          settle
///   Funded ──────────────> Claimed ────────> Attested ──────> Settled
///     │                       │                  │
///     │ refund                │ dispute          │ dispute
///     │ (expired or           ▼                  ▼
///     │  clinic inactive)  Disputed <────────────┘
///     ▼                       │
///  Refunded  <──── resolve ───┴──── resolve ────> Settled
///     ▲
///     └── refund (claimed, never attested, past expiry + claim grace)
/// ```
///
/// `Settled` and `Refunded` are terminal.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoucherStatus {
    Funded = 0,
    Claimed = 1,
    Attested = 2,
    Settled = 3,
    Disputed = 4,
    Refunded = 5,
}

/// An escrowed commitment to pay one provider for one service.
///
/// The funder is whoever paid — the patient themselves, or a family member
/// sending from abroad. `beneficiary_ref` is an opaque salted hash of an
/// off-chain patient identifier; it carries no clinical meaning and cannot
/// be reversed into an identity from on-chain data alone.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Voucher {
    pub id: u64,
    pub funder: Address,
    pub beneficiary_ref: BytesN<32>,
    pub provider: Address,
    pub service_code: u32,
    pub amount: i128,
    pub status: VoucherStatus,
    pub created_at: u64,
    pub expires_at: u64,
    pub claimed_at: u64,
    pub attested_at: u64,
    /// Earliest ledger time at which `settle` may run. Zero until attested.
    pub dispute_deadline: u64,
    /// Attester who confirmed delivery, recorded by `attest`.
    ///
    /// `None` until attestation. Settlement pays this address its share of
    /// the protocol fee, which is why it is stored on the voucher rather than
    /// looked up later: the attester of record is the one who did the work.
    pub attester: Option<Address>,
}

/// Protocol-level settings, fixed at initialization.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub admin: Address,
    pub registry: Address,
    pub receipt_book: Address,
    /// Settlement token. USDC on Stellar in production.
    pub token: Address,
    /// Seconds a funder has to dispute after attestation.
    pub dispute_window: u64,
    /// Protocol fee in basis points. 100 = 1%.
    pub fee_bps: u32,
    /// Share of the voucher amount paid to the attester, in basis points.
    ///
    /// Carved out of `fee_bps`, so it can never exceed the protocol fee:
    /// the clinic's net is unchanged, the attester receives
    /// `amount * attester_share_bps / 10_000`, and the fee account receives
    /// the rest. Zero means attesters are not paid at settlement, which is
    /// exactly the pre-attester-share behaviour.
    pub attester_share_bps: u32,
    pub fee_account: Address,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Config,
    NextId,
    Voucher(u64),
    /// Address nominated by `propose_admin`, awaiting `accept_admin`.
    PendingAdmin,
    /// Most attestations one attester may make per UTC day. Unset means
    /// `DEFAULT_MAX_ATTESTATIONS_PER_DAY`.
    AttestationCap,
    /// Attestations made by an attester on a UTC day (`timestamp / 86400`).
    /// Temporary storage: it only has to outlive the day it counts.
    Attestations(Address, u64),
    /// Total amount held in escrow by vouchers that are not yet settled or
    /// refunded. Kept in instance storage and updated on every movement of
    /// funds, so `escrow_summary` can report it without scanning vouchers.
    OpenEscrow,
}

#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum VoucherError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    NotAuthorized = 3,
    VoucherNotFound = 4,
    ProviderNotActive = 5,
    ServiceNotOffered = 6,
    AmountBelowPrice = 7,
    InvalidAmount = 8,
    InvalidExpiry = 9,
    InvalidStatus = 10,
    VoucherExpired = 11,
    NotYetExpired = 12,
    DisputeWindowOpen = 13,
    DisputeWindowClosed = 14,
    InvalidFee = 15,
    MathOverflow = 16,
    NoPendingAdmin = 17,
    /// The clinic has paused this service; it cannot be funded until it
    /// is made available again.
    ServiceUnavailable = 18,
    /// This attester has used up today's attestations; try again tomorrow
    /// (UTC) or ask the admin to raise the cap.
    AttestationCapReached = 19,
    /// The attestation cap must be at least 1.
    InvalidAttestationCap = 20,
    /// `attester_share_bps` was greater than `fee_bps`. The attester's share
    /// is carved out of the protocol fee, so it cannot exceed it.
    InvalidAttesterShare = 21,
    /// The new amount is not lower than the amount already escrowed, so
    /// there is nothing to give back.
    AmountNotReduced = 22,
    /// The contract holds less of the settlement token than this voucher
    /// escrows. Happens only if the token issuer clawed escrowed funds back.
    EscrowShortfall = 23,
    /// The token refused the payment, typically because the issuer froze
    /// (deauthorized) the escrow or the recipient. Nothing moved; the voucher
    /// is unchanged and the call can be retried once the freeze is lifted.
    TokenTransferFailed = 24,
}

/// Maps the shared helpers' failures onto this contract's error numbers, so
/// callers keep seeing the same codes they did before the helpers were shared.
impl From<sci_common::CommonError> for VoucherError {
    fn from(error: sci_common::CommonError) -> Self {
        match error {
            sci_common::CommonError::NotAuthorized => VoucherError::NotAuthorized,
            sci_common::CommonError::NoPendingAdmin => VoucherError::NoPendingAdmin,
        }
    }
}
