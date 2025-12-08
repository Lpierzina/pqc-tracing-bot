use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use core::fmt; // used only when std? but for Display? We'll keep.

/// HQC parameter sets exposed through liboqs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HqcLevel {
    /// HQC-128: ≈128-bit post-quantum security.
    Hqc128,
    /// HQC-192: ≈192-bit post-quantum security.
    Hqc192,
    /// HQC-256: ≈256-bit post-quantum security.
    Hqc256,
}

impl HqcLevel {
    /// Provides the advertised security strength in bits.
    pub const fn security_bits(self) -> u16 {
        match self {
            Self::Hqc128 => 128,
            Self::Hqc192 => 192,
            Self::Hqc256 => 256,
        }
    }

    /// Human-readable descriptor for docs/logging.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Hqc128 => "HQC-128",
            Self::Hqc192 => "HQC-192",
            Self::Hqc256 => "HQC-256",
        }
    }
}

/// HQC keypair container returned by the engine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HqcKeyPair {
    pub public_key: Vec<u8>,
    pub secret_key: Vec<u8>,
    pub level: HqcLevel,
}

/// HQC encapsulation output (ciphertext + shared secret).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HqcEncapsulation {
    pub ciphertext: Vec<u8>,
    pub shared_secret: Vec<u8>,
}

/// Errors surfaced by the HQC engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HqcError {
    InvalidInput(&'static str),
    IntegrationError(&'static str, String),
}

impl HqcError {
    pub(crate) fn integration(context: &'static str, err: impl fmt::Display) -> Self {
        Self::IntegrationError(context, err.to_string())
    }
}

/// Result alias for HQC operations.
pub type HqcResult<T> = core::result::Result<T, HqcError>;
