//! Abstractions for ML-KEM (Kyber) engines.

use crate::error::PqcResult;
use crate::types::{Bytes, SecurityLevel};
use alloc::boxed::Box;

/// NIST ML-KEM key pair produced by the host engine.
#[derive(Clone, Debug)]
pub struct MlKemKeyPair {
    /// Public key bytes serialized per FIPS 203.
    pub public_key: Bytes,
    /// Secret key bytes serialized per FIPS 203.
    pub secret_key: Bytes,
    /// Security level (e.g., ML-KEM-128/192/256).
    pub level: SecurityLevel,
}

/// Ciphertext + shared secret from ML-KEM encapsulation.
#[derive(Clone, Debug)]
pub struct MlKemEncapsulation {
    /// Kyber ciphertext.
    pub ciphertext: Bytes,
    /// Shared secret derived during encapsulation.
    pub shared_secret: Bytes,
}

/// Abstract ML-KEM interface that delegates to a host implementation.
///
/// Implementations must:
/// - use NIST-compliant ML-KEM per FIPS 203,
/// - provide constant-time behavior,
/// - pass KATs and IND-CCA2 proofs in the host test suite.
pub trait MlKem: Send + Sync {
    /// Return the configured security level.
    fn level(&self) -> SecurityLevel;

    /// Generate a fresh ML-KEM key pair.
    fn keygen(&self) -> PqcResult<MlKemKeyPair>;

    /// Encapsulate to the provided public key.
    fn encapsulate(&self, public_key: &[u8]) -> PqcResult<MlKemEncapsulation>;

    /// Decapsulate ciphertext with the provided secret key.
    fn decapsulate(&self, secret_key: &[u8], ciphertext: &[u8]) -> PqcResult<Bytes>;
}

/// Thin wrapper used by contracts to access the host ML-KEM engine.
pub struct MlKemEngine {
    inner: Box<dyn MlKem>,
}

impl MlKemEngine {
    /// Create a new engine wrapper.
    pub fn new(inner: Box<dyn MlKem>) -> Self {
        Self { inner }
    }

    /// Generate a fresh ML-KEM key pair.
    pub fn keygen(&self) -> PqcResult<MlKemKeyPair> {
        self.inner.keygen()
    }

    /// Encapsulate to the provided public key.
    pub fn encapsulate(&self, pk: &[u8]) -> PqcResult<MlKemEncapsulation> {
        self.inner.encapsulate(pk)
    }

    /// Decapsulate using the provided secret key and ciphertext.
    pub fn decapsulate(&self, sk: &[u8], ct: &[u8]) -> PqcResult<Bytes> {
        self.inner.decapsulate(sk, ct)
    }
}
