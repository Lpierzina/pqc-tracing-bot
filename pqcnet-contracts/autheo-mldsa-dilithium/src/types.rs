use alloc::{string::String, vec::Vec};
#[cfg(feature = "liboqs")]
use alloc::string::ToString;
#[cfg(feature = "liboqs")]
use core::fmt;

/// Supported Dilithium (ML-DSA) levels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DilithiumLevel {
    /// ML-DSA-44 (~128-bit PQ security).
    MlDsa44,
    /// ML-DSA-65 (~192-bit PQ security).
    MlDsa65,
    /// ML-DSA-87 (~256-bit PQ security).
    MlDsa87,
}

/// Dilithium keypair container.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DilithiumKeyPair {
    pub public_key: Vec<u8>,
    pub secret_key: Vec<u8>,
    pub level: DilithiumLevel,
}

/// Errors returned by the Dilithium adapters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DilithiumError {
    InvalidInput(&'static str),
    VerifyFailed,
    IntegrationError(&'static str, String),
}

#[cfg(feature = "liboqs")]
impl DilithiumError {
    pub(crate) fn integration(context: &'static str, err: impl fmt::Display) -> Self {
        Self::IntegrationError(context, err.to_string())
    }
}

/// Result alias for Dilithium operations.
pub type DilithiumResult<T> = core::result::Result<T, DilithiumError>;
