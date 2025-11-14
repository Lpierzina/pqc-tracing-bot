use alloc::string::String;

/// Unified error type for PQCNet contracts.
///
/// The host runtime can map each variant to standardized numeric codes
/// (e.g., `PQC-001`) when exposing errors externally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PqcError {
    /// Underlying PQC primitive failed or returned invalid data.
    PrimitiveFailure(&'static str),
    /// Invalid input (e.g., length, encoding, or parameters).
    InvalidInput(&'static str),
    /// Threshold sharing failed (e.g., insufficient shares).
    ThresholdFailure(&'static str),
    /// Signature verification failed.
    VerifyFailed,
    /// Operation exceeded configured limits (e.g., batch size).
    LimitExceeded(&'static str),
    /// Integration-layer error (QS-DAG, storage, or host).
    IntegrationError(String),
    /// Generic catch-all error.
    InternalError(&'static str),
}

/// Alias for fallible PQC operations.
pub type PqcResult<T> = core::result::Result<T, PqcError>;
