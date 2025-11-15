#![no_std]

//! Deterministic Dilithium3 (ML-DSA) demo engine for PQC core integration tests.

extern crate alloc;

use alloc::vec::Vec;
use blake2::Blake2s256;
use digest::Digest;
use spin::Mutex;

const DOMAIN_MLDSA_SK: &[u8] = b"PQCNET_MLDSA_SK_V1";
const DOMAIN_MLDSA_PK: &[u8] = b"PQCNET_MLDSA_PK_V1";
const DOMAIN_MLDSA_SIG: &[u8] = b"PQCNET_MLDSA_SIG_V1";

/// Supported Dilithium level tags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DilithiumLevel {
    /// Dilithium3 / ML-DSA-65.
    MlDsa65,
}

/// Deterministic Dilithium3 engine.
pub struct DilithiumDeterministic {
    counter: Mutex<u64>,
}

impl DilithiumDeterministic {
    /// Create a new deterministic engine.
    pub const fn new() -> Self {
        Self {
            counter: Mutex::new(7),
        }
    }

    /// Level tag reported by this engine.
    pub const fn level(&self) -> DilithiumLevel {
        DilithiumLevel::MlDsa65
    }

    /// Generate a deterministic ML-DSA keypair.
    pub fn keypair(&self) -> DilithiumResult<DilithiumKeyPair> {
        let secret_seed = self.next_seed();
        let secret_key = secret_seed.to_vec();
        let public_key = expand_bytes(DOMAIN_MLDSA_PK, &secret_seed, 32);

        Ok(DilithiumKeyPair {
            public_key,
            secret_key,
            level: self.level(),
        })
    }

    /// Sign arbitrary data.
    pub fn sign(&self, secret_key: &[u8], message: &[u8]) -> DilithiumResult<Vec<u8>> {
        if secret_key.is_empty() {
            return Err(DilithiumError::InvalidInput("ml-dsa secret missing"));
        }

        let public_key = expand_bytes(DOMAIN_MLDSA_PK, secret_key, 32);
        let mut transcript =
            Vec::with_capacity(public_key.len() + message.len() + DOMAIN_MLDSA_SIG.len());
        transcript.extend_from_slice(DOMAIN_MLDSA_SIG);
        transcript.extend_from_slice(&public_key);
        transcript.extend_from_slice(message);

        let mut digest = Blake2s256::new();
        digest.update(&transcript);
        Ok(digest.finalize().to_vec())
    }

    /// Verify the signature produced by [`Self::sign`].
    pub fn verify(
        &self,
        public_key: &[u8],
        message: &[u8],
        signature: &[u8],
    ) -> DilithiumResult<()> {
        if public_key.is_empty() {
            return Err(DilithiumError::InvalidInput("ml-dsa pk missing"));
        }
        if signature.len() != 32 {
            return Err(DilithiumError::InvalidInput(
                "ml-dsa signature length invalid",
            ));
        }

        let mut transcript =
            Vec::with_capacity(public_key.len() + message.len() + DOMAIN_MLDSA_SIG.len());
        transcript.extend_from_slice(DOMAIN_MLDSA_SIG);
        transcript.extend_from_slice(public_key);
        transcript.extend_from_slice(message);

        let mut digest = Blake2s256::new();
        digest.update(&transcript);
        let expected = digest.finalize();

        if expected.as_slice() == signature {
            Ok(())
        } else {
            Err(DilithiumError::VerifyFailed)
        }
    }

    fn next_seed(&self) -> [u8; 32] {
        let mut guard = self.counter.lock();
        let current = *guard;
        *guard = current.wrapping_add(1);
        drop(guard);

        let mut seed = [0u8; 32];
        let derived = expand_bytes(DOMAIN_MLDSA_SK, &current.to_le_bytes(), 32);
        seed.copy_from_slice(&derived);
        seed
    }
}

impl Default for DilithiumDeterministic {
    fn default() -> Self {
        Self::new()
    }
}

/// Dilithium keypair container.
#[derive(Clone)]
pub struct DilithiumKeyPair {
    pub public_key: Vec<u8>,
    pub secret_key: Vec<u8>,
    pub level: DilithiumLevel,
}

/// Error type returned by the deterministic Dilithium engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DilithiumError {
    InvalidInput(&'static str),
    VerifyFailed,
}

/// Result alias for Dilithium operations.
pub type DilithiumResult<T> = core::result::Result<T, DilithiumError>;

fn expand_bytes(domain: &[u8], input: &[u8], len: usize) -> Vec<u8> {
    if len == 0 {
        return Vec::new();
    }

    let mut out = Vec::with_capacity(len);
    let mut counter: u32 = 0;

    while out.len() < len {
        let mut digest = Blake2s256::new();
        digest.update(domain);
        digest.update(&(len as u32).to_le_bytes());
        digest.update(input);
        digest.update(&counter.to_le_bytes());

        let block = digest.finalize();
        let remaining = len - out.len();
        let chunk = &block[..remaining.min(block.len())];
        out.extend_from_slice(chunk);
        counter = counter.wrapping_add(1);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keypair_and_signature_lengths_match_expectations() {
        let engine = DilithiumDeterministic::new();
        let pair = engine.keypair().expect("keypair");
        assert_eq!(pair.public_key.len(), 32);
        assert_eq!(pair.secret_key.len(), 32);
        assert_eq!(pair.level, DilithiumLevel::MlDsa65);

        let sig = engine
            .sign(&pair.secret_key, b"hello world")
            .expect("sign");
        assert_eq!(sig.len(), 32);
    }

    #[test]
    fn sign_and_verify_roundtrip_succeeds() {
        let engine = DilithiumDeterministic::new();
        let pair = engine.keypair().expect("keypair");
        let message = b"autheo pqc handshake";
        let sig = engine.sign(&pair.secret_key, message).expect("sign");
        engine
            .verify(&pair.public_key, message, &sig)
            .expect("verify");
    }

    #[test]
    fn sign_rejects_empty_secret_key() {
        let engine = DilithiumDeterministic::new();
        let err = engine.sign(&[], b"msg").unwrap_err();
        assert_eq!(err, DilithiumError::InvalidInput("ml-dsa secret missing"));
    }

    #[test]
    fn verify_rejects_bad_signature_length() {
        let engine = DilithiumDeterministic::new();
        let err = engine.verify(&[1u8; 32], b"msg", &[0u8; 16]).unwrap_err();
        assert_eq!(
            err,
            DilithiumError::InvalidInput("ml-dsa signature length invalid")
        );
    }

    #[test]
    fn verify_detects_tampering() {
        let engine = DilithiumDeterministic::new();
        let pair = engine.keypair().expect("keypair");
        let message = b"real message";
        let mut sig = engine.sign(&pair.secret_key, message).expect("sign");
        sig[0] ^= 0xFF;
        let err = engine.verify(&pair.public_key, message, &sig).unwrap_err();
        assert_eq!(err, DilithiumError::VerifyFailed);
    }
}
