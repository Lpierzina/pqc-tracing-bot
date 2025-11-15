#![no_std]

//! Deterministic Kyber (ML-KEM-768) demo engine used by Autheo PQC core tests.
//!
//! The real Autheo deployments swap this module for audited Kyber bindings.

extern crate alloc;

use alloc::vec::Vec;
use blake2::Blake2s256;
use digest::Digest;
use spin::Mutex;

const DOMAIN_MLKEM_SK: &[u8] = b"PQCNET_MLKEM_SK_V1";
const DOMAIN_MLKEM_PK: &[u8] = b"PQCNET_MLKEM_PK_V1";
const DOMAIN_MLKEM_CT: &[u8] = b"PQCNET_MLKEM_CT_V1";
const DOMAIN_MLKEM_SS: &[u8] = b"PQCNET_MLKEM_SS_V1";

/// Supported Kyber (ML-KEM) security levels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KyberLevel {
    /// ML-KEM-768 (roughly 192-bit PQ security).
    MlKem768,
}

/// Deterministic ML-KEM-768 engine used for demos/tests.
pub struct KyberDeterministic {
    counter: Mutex<u64>,
}

impl KyberDeterministic {
    /// Create a new deterministic engine.
    pub const fn new() -> Self {
        Self {
            counter: Mutex::new(1),
        }
    }

    /// Return the configured Kyber level.
    pub const fn level(&self) -> KyberLevel {
        KyberLevel::MlKem768
    }

    /// Generate a deterministic key pair.
    pub fn keypair(&self) -> KyberResult<KyberKeyPair> {
        let secret_seed = self.next_seed();
        let secret_key = secret_seed.to_vec();
        let public_key = expand_bytes(DOMAIN_MLKEM_PK, &secret_seed, 32);

        Ok(KyberKeyPair {
            public_key,
            secret_key,
            level: self.level(),
        })
    }

    /// Encapsulate to the supplied Kyber public key.
    pub fn encapsulate(&self, public_key: &[u8]) -> KyberResult<KyberEncapsulation> {
        if public_key.is_empty() {
            return Err(KyberError::InvalidInput("ml-kem pk missing"));
        }

        let ciphertext = expand_bytes(DOMAIN_MLKEM_CT, public_key, 48);
        let shared_secret = expand_bytes(DOMAIN_MLKEM_SS, &ciphertext, 32);

        Ok(KyberEncapsulation {
            ciphertext,
            shared_secret,
        })
    }

    /// Decapsulate Kyber ciphertext using the provided secret key.
    pub fn decapsulate(&self, _secret_key: &[u8], ciphertext: &[u8]) -> KyberResult<Vec<u8>> {
        if ciphertext.is_empty() {
            return Err(KyberError::InvalidInput("ml-kem ciphertext missing"));
        }
        Ok(expand_bytes(DOMAIN_MLKEM_SS, ciphertext, 32))
    }

    fn next_seed(&self) -> [u8; 32] {
        let mut guard = self.counter.lock();
        let current = *guard;
        *guard = current.wrapping_add(1);
        drop(guard);

        let mut seed = [0u8; 32];
        let derived = expand_bytes(DOMAIN_MLKEM_SK, &current.to_le_bytes(), 32);
        seed.copy_from_slice(&derived);
        seed
    }
}

impl Default for KyberDeterministic {
    fn default() -> Self {
        Self::new()
    }
}

/// Kyber key pair (ML-KEM-768 serialization).
#[derive(Clone)]
pub struct KyberKeyPair {
    pub public_key: Vec<u8>,
    pub secret_key: Vec<u8>,
    pub level: KyberLevel,
}

/// Kyber encapsulation result.
#[derive(Clone)]
pub struct KyberEncapsulation {
    pub ciphertext: Vec<u8>,
    pub shared_secret: Vec<u8>,
}

/// Errors returned by the deterministic Kyber engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KyberError {
    InvalidInput(&'static str),
}

/// Kyber result alias.
pub type KyberResult<T> = core::result::Result<T, KyberError>;

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
