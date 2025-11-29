use alloc::{string::String, vec::Vec};
#[cfg(feature = "liboqs")]
use alloc::string::ToString;
#[cfg(feature = "liboqs")]
use core::fmt;

/// Supported Kyber (ML-KEM) levels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KyberLevel {
    /// ML-KEM-512 (~128-bit PQ security).
    MlKem512,
    /// ML-KEM-768 (~192-bit PQ security).
    MlKem768,
    /// ML-KEM-1024 (~256-bit PQ security).
    MlKem1024,
}

/// Kyber key pair container.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KyberKeyPair {
    pub public_key: Vec<u8>,
    pub secret_key: Vec<u8>,
    pub level: KyberLevel,
}

/// Kyber encapsulation result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KyberEncapsulation {
    pub ciphertext: Vec<u8>,
    pub shared_secret: Vec<u8>,
}

/// Errors surfaced by the Kyber adapters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KyberError {
    InvalidInput(&'static str),
    IntegrationError(&'static str, String),
}

#[cfg(feature = "liboqs")]
impl KyberError {
    pub(crate) fn integration(context: &'static str, err: impl fmt::Display) -> Self {
        Self::IntegrationError(context, err.to_string())
    }
}

/// Result alias for Kyber operations.
pub type KyberResult<T> = core::result::Result<T, KyberError>;
