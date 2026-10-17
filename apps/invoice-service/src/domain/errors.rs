use thiserror::Error;

#[derive(Debug, Error)]
pub enum InvoiceError {
    #[error("invoice not found")]
    InvoiceNotFound,
    #[error("invoice is not draft")]
    InvoiceNotDraft,
    #[error("invoice is not open")]
    InvoiceNotOpen,
    #[error("invoice already paid")]
    InvoiceAlreadyPaid,
    #[error("invoice is voided")]
    InvoiceVoided,
    #[error("invalid period")]
    InvalidPeriod,
    #[error("pdf generation failed: {0}")]
    PdfGenerationFailed(String),
    #[error("pdf storage failed: {0}")]
    PdfStorageFailed(String),
    #[error("validation error: {0}")]
    Validation(String),
    #[error("dependency unavailable: {0}")]
    DependencyUnavailable(String),
}
