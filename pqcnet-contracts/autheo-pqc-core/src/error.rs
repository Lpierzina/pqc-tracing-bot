use alloc::string::String;
use core::fmt;
use pqcnet_qace::QaceError;

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

impl fmt::Display for PqcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PqcError::PrimitiveFailure(msg) => write!(f, "primitive failure: {msg}"),
            PqcError::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
            PqcError::ThresholdFailure(msg) => write!(f, "threshold failure: {msg}"),
            PqcError::VerifyFailed => write!(f, "verification failed"),
            PqcError::LimitExceeded(msg) => write!(f, "limit exceeded: {msg}"),
            PqcError::IntegrationError(msg) => write!(f, "integration error: {msg}"),
            PqcError::InternalError(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl std::error::Error for PqcError {}

impl From<QaceError> for PqcError {
    fn from(err: QaceError) -> Self {
        match err {
            QaceError::InvalidInput(msg) => PqcError::InvalidInput(msg),
            QaceError::IntegrationError(msg) => PqcError::IntegrationError(msg.into()),
        }
    }
}
