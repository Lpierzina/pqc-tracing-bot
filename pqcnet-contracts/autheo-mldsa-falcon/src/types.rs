use alloc::{string::String, vec::Vec};
#[cfg(feature = "liboqs")]
use alloc::string::ToString;
#[cfg(feature = "liboqs")]
use core::fmt;

/// Supported Falcon security levels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FalconLevel {
    /// Falcon-512 (~128-bit PQ security).
    Falcon512,
    /// Falcon-1024 (~192-bit PQ security).
    Falcon1024,
}

/// Falcon keypair container.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FalconKeyPair {
    pub public_key: Vec<u8>,
    pub secret_key: Vec<u8>,
    pub level: FalconLevel,
}

/// Errors returned by the Falcon adapters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FalconError {
    InvalidInput(&'static str),
    VerifyFailed,
    IntegrationError(&'static str, String),
}

#[cfg(feature = "liboqs")]
impl FalconError {
    pub(crate) fn integration(context: &'static str, err: impl fmt::Display) -> Self {
        Self::IntegrationError(context, err.to_string())
    }
}

/// Result alias for Falcon operations.
pub type FalconResult<T> = core::result::Result<T, FalconError>;
