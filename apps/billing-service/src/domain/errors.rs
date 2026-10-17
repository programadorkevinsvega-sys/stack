use thiserror::Error;

#[derive(Debug, Error)]
pub enum BillingError {
    #[error("wallet not found")]
    WalletNotFound,
    #[error("reservation not found")]
    ReservationNotFound,
    #[error("insufficient balance")]
    InsufficientBalance,
    #[error("reservation is not pending")]
    ReservationNotPending,
    #[error("ledger entry already exists")]
    DuplicateLedgerEntry,
    #[error("wallet ledger integrity error: {0}")]
    WalletLedgerIntegrityError(String),
    #[error("{0}")]
    Validation(String),
    #[error("dependency unavailable: {0}")]
    DependencyUnavailable(String),
}
