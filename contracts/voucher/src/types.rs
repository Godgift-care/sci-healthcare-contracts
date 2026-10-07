use soroban_sdk::{contracterror, contracttype, Address, BytesN};

/// Lifecycle of a single care voucher.
///
/// ```text
///                  claim            attest          settle
///   Funded ──────────────> Claimed ────────> Attested ──────> Settled
///     │                       │                  │
///     │ refund (past expiry)  │ dispute          │ dispute
///     ▼                       ▼                  ▼
///  Refunded  <──────────  Disputed  ────────────────> (resolve)
///     ▲                       │
///     └───────────────────────┘
///       refund (claimed, never attested, past expiry + claim grace)
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
}
