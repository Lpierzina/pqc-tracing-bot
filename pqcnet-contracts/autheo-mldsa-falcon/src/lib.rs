#![no_std]

//! Deterministic Falcon (ML-DSA alternative) engine placeholder.

extern crate alloc;

use alloc::vec::Vec;
use blake2::Blake2s256;
use digest::Digest;
use spin::Mutex;

const DOMAIN_FALCON_SK: &[u8] = b"PQCNET_FALCON_SK_V1";
const DOMAIN_FALCON_PK: &[u8] = b"PQCNET_FALCON_PK_V1";
const DOMAIN_FALCON_SIG: &[u8] = b"PQCNET_FALCON_SIG_V1";

/// Supported Falcon level tags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FalconLevel {
    /// Falcon-1024 style security.
    Falcon1024,
}

/// Deterministic Falcon engine used for demos and integration tests.
pub struct FalconDeterministic {
    counter: Mutex<u64>,
}

impl FalconDeterministic {
    /// Create a new deterministic Falcon engine.
    pub const fn new() -> Self {
        Self {
            counter: Mutex::new(11),
        }
    }

    /// Return the configured Falcon security level.
    pub const fn level(&self) -> FalconLevel {
        FalconLevel::Falcon1024
    }

    /// Generate a deterministic Falcon keypair.
    pub fn keypair(&self) -> FalconResult<FalconKeyPair> {
        let seed = self.next_seed();
        let secret_key = seed.to_vec();
        let public_key = expand_bytes(DOMAIN_FALCON_PK, &seed, 32);

        Ok(FalconKeyPair {
            public_key,
            secret_key,
            level: self.level(),
        })
    }

    /// Sign data with the Falcon secret key.
    pub fn sign(&self, secret_key: &[u8], message: &[u8]) -> FalconResult<Vec<u8>> {
        if secret_key.is_empty() {
            return Err(FalconError::InvalidInput("falcon secret missing"));
        }

        let public_key = expand_bytes(DOMAIN_FALCON_PK, secret_key, 32);
        let mut transcript =
            Vec::with_capacity(public_key.len() + message.len() + DOMAIN_FALCON_SIG.len());
        transcript.extend_from_slice(DOMAIN_FALCON_SIG);
        transcript.extend_from_slice(&public_key);
        transcript.extend_from_slice(message);

        let mut digest = Blake2s256::new();
        digest.update(&transcript);
        Ok(digest.finalize().to_vec())
    }

    /// Verify a Falcon signature.
    pub fn verify(&self, public_key: &[u8], message: &[u8], signature: &[u8]) -> FalconResult<()> {
        if public_key.is_empty() {
            return Err(FalconError::InvalidInput("falcon pk missing"));
        }
        if signature.len() != 32 {
            return Err(FalconError::InvalidInput("falcon signature length invalid"));
        }

        let mut transcript =
            Vec::with_capacity(public_key.len() + message.len() + DOMAIN_FALCON_SIG.len());
        transcript.extend_from_slice(DOMAIN_FALCON_SIG);
        transcript.extend_from_slice(public_key);
        transcript.extend_from_slice(message);

        let mut digest = Blake2s256::new();
        digest.update(&transcript);
        let expected = digest.finalize();

        if expected.as_slice() == signature {
            Ok(())
        } else {
            Err(FalconError::VerifyFailed)
        }
    }

    fn next_seed(&self) -> [u8; 32] {
        let mut guard = self.counter.lock();
        let current = *guard;
        *guard = current.wrapping_add(1);
        drop(guard);

        let mut seed = [0u8; 32];
        let derived = expand_bytes(DOMAIN_FALCON_SK, &current.to_le_bytes(), 32);
        seed.copy_from_slice(&derived);
        seed
    }
}

impl Default for FalconDeterministic {
    fn default() -> Self {
        Self::new()
    }
}

/// Falcon keypair container.
#[derive(Clone)]
pub struct FalconKeyPair {
    pub public_key: Vec<u8>,
    pub secret_key: Vec<u8>,
    pub level: FalconLevel,
}

/// Errors returned by the deterministic Falcon engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FalconError {
    InvalidInput(&'static str),
    VerifyFailed,
}

/// Result alias for Falcon operations.
pub type FalconResult<T> = core::result::Result<T, FalconError>;

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
