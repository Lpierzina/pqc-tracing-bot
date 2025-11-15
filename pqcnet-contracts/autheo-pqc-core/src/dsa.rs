//! Abstractions for ML-DSA (Dilithium) engines.

use crate::error::{PqcError, PqcResult};
use crate::types::{Bytes, SecurityLevel};
use alloc::boxed::Box;

/// ML-DSA key pair produced by the host engine.
#[derive(Clone)]
pub struct MlDsaKeyPair {
    /// Public key bytes serialized per FIPS 204.
    pub public_key: Bytes,
    /// Secret key bytes serialized per FIPS 204.
    pub secret_key: Bytes,
    /// Security level (e.g., ML-DSA-44/65/87).
    pub level: SecurityLevel,
}

/// Trait describing the host ML-DSA implementation.
pub trait MlDsa: Send + Sync {
    /// Return the configured security level.
    fn level(&self) -> SecurityLevel;

    /// Generate a fresh ML-DSA key pair.
    fn keygen(&self) -> PqcResult<MlDsaKeyPair>;

    /// Produce a signature with the supplied secret key.
    fn sign(&self, sk: &[u8], message: &[u8]) -> PqcResult<Bytes>;

    /// Verify the signature using the provided public key.
    fn verify(&self, pk: &[u8], message: &[u8], signature: &[u8]) -> PqcResult<()>;
}

/// Thin wrapper used by contract logic to call ML-DSA engines.
pub struct MlDsaEngine {
    inner: Box<dyn MlDsa>,
}

impl MlDsaEngine {
    /// Create a new engine wrapper.
    pub fn new(inner: Box<dyn MlDsa>) -> Self {
        Self { inner }
    }

    /// Generate a fresh ML-DSA key pair.
    pub fn keygen(&self) -> PqcResult<MlDsaKeyPair> {
        self.inner.keygen()
    }

    /// Sign arbitrary data.
    pub fn sign(&self, sk: &[u8], msg: &[u8]) -> PqcResult<Bytes> {
        self.inner.sign(sk, msg)
    }

    /// Verify a signature.
    pub fn verify(&self, pk: &[u8], msg: &[u8], sig: &[u8]) -> PqcResult<()> {
        self.inner.verify(pk, msg, sig)
    }

    /// Batch verify up to `max_batch` signatures via repeated calls to the engine.
    pub fn batch_verify(
        &self,
        max_batch: usize,
        pks: &[Bytes],
        messages: &[Bytes],
        signatures: &[Bytes],
    ) -> PqcResult<()> {
        let n = pks.len();
        if n == 0 || n != messages.len() || n != signatures.len() {
            return Err(PqcError::InvalidInput("batch dimension mismatch"));
        }
        if n > max_batch {
            return Err(PqcError::LimitExceeded("batch too large"));
        }

        for i in 0..n {
            self.verify(&pks[i], &messages[i], &signatures[i])?;
        }
        Ok(())
    }
}
