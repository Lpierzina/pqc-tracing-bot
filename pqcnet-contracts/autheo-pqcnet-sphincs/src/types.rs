#[cfg(feature = "liboqs")]
use alloc::string::ToString;
use alloc::{string::String, vec::Vec};
#[cfg(feature = "liboqs")]
use core::fmt;

/// Supported FIPS 205 SPHINCS+ (SLH-DSA) parameter sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SphincsPlusSecurityLevel {
    /// SLH-DSA-SHAKE-128s (small signatures, 128-bit security).
    Shake128s,
    /// SLH-DSA-SHAKE-128f (fast, 128-bit security).
    Shake128f,
    /// SLH-DSA-SHAKE-192s (small signatures, 192-bit security).
    Shake192s,
    /// SLH-DSA-SHAKE-192f (fast, 192-bit security).
    Shake192f,
    /// SLH-DSA-SHAKE-256s (small signatures, 256-bit security).
    Shake256s,
    /// SLH-DSA-SHAKE-256f (fast, 256-bit security).
    Shake256f,
}

impl SphincsPlusSecurityLevel {
    /// Claimed post-quantum security strength in bits.
    pub const fn security_bits(self) -> u16 {
        match self {
            Self::Shake128s | Self::Shake128f => 128,
            Self::Shake192s | Self::Shake192f => 192,
            Self::Shake256s | Self::Shake256f => 256,
        }
    }

    /// Canonical public-key length for this parameter set.
    pub const fn public_key_len(self) -> usize {
        match self {
            Self::Shake128s | Self::Shake128f => 32,
            Self::Shake192s | Self::Shake192f => 48,
            Self::Shake256s | Self::Shake256f => 64,
        }
    }

    /// Canonical secret-key length for this parameter set.
    pub const fn secret_key_len(self) -> usize {
        match self {
            Self::Shake128s | Self::Shake128f => 64,
            Self::Shake192s | Self::Shake192f => 96,
            Self::Shake256s | Self::Shake256f => 128,
        }
    }

    /// Deterministic signature length (bytes) for this parameter set.
    pub const fn signature_len(self) -> usize {
        match self {
            Self::Shake128f => 17_088,
            Self::Shake128s => 7_856,
            Self::Shake192f => 35_664,
            Self::Shake192s => 16_224,
            Self::Shake256f => 49_856,
            Self::Shake256s => 29_792,
        }
    }

    /// Byte tag used for deterministic domain separation.
    pub(crate) const fn domain_tag(self) -> u8 {
        match self {
            Self::Shake128s => 0x01,
            Self::Shake128f => 0x02,
            Self::Shake192s => 0x03,
            Self::Shake192f => 0x04,
            Self::Shake256s => 0x05,
            Self::Shake256f => 0x06,
        }
    }
}

/// SPHINCS+ keypair container.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SphincsPlusKeyPair {
    pub public_key: Vec<u8>,
    pub secret_key: Vec<u8>,
    pub level: SphincsPlusSecurityLevel,
}

/// Errors returned by the SPHINCS+ adapters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SphincsPlusError {
    InvalidInput(&'static str),
    VerifyFailed,
    IntegrationError(&'static str, String),
}

#[cfg(feature = "liboqs")]
impl SphincsPlusError {
    pub(crate) fn integration(context: &'static str, err: impl fmt::Display) -> Self {
        Self::IntegrationError(context, err.to_string())
    }
}

/// Result alias for SPHINCS+ operations.
pub type SphincsPlusResult<T> = core::result::Result<T, SphincsPlusError>;
