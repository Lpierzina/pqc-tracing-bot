#![cfg(feature = "deterministic")]

use crate::types::{
    SphincsPlusError, SphincsPlusKeyPair, SphincsPlusResult, SphincsPlusSecurityLevel,
};
use alloc::vec::Vec;
use blake2::Blake2s256;
use digest::{Digest, Output};
use spin::Mutex;

const DOMAIN_SK: &[u8] = b"PQCNET_SPHINCS_SK_V1";
const DOMAIN_PK: &[u8] = b"PQCNET_SPHINCS_PK_V1";
const DOMAIN_SIG: &[u8] = b"PQCNET_SPHINCS_SIG_V1";
const DOMAIN_SEED: &[u8] = b"PQCNET_SPHINCS_SEED_V1";
const DETERMINISTIC_SEED_LEN: usize = 32;

/// Deterministic SPHINCS+ fallback, mirroring the SLH-DSA interface.
pub struct SphincsPlusDeterministic {
    counter: Mutex<u64>,
    level: SphincsPlusSecurityLevel,
}

impl SphincsPlusDeterministic {
    /// Create a deterministic engine for the provided parameter set.
    pub const fn new(level: SphincsPlusSecurityLevel) -> Self {
        Self {
            counter: Mutex::new(0x5A_51_6C_65_55_53_10),
            level,
        }
    }

    /// Return the configured SPHINCS+ level.
    pub const fn level(&self) -> SphincsPlusSecurityLevel {
        self.level
    }

    /// Generate a deterministic keypair.
    pub fn keypair(&self) -> SphincsPlusResult<SphincsPlusKeyPair> {
        let seed = self.next_seed();
        let public_key = expand_bytes(DOMAIN_PK, &seed, self.level.public_key_len());
        let mut secret_key = Vec::with_capacity(self.level.secret_key_len());
        secret_key.extend_from_slice(&seed);
        if self.level.secret_key_len() > DETERMINISTIC_SEED_LEN {
            let supplement = expand_bytes(
                DOMAIN_SK,
                &seed,
                self.level.secret_key_len() - DETERMINISTIC_SEED_LEN,
            );
            secret_key.extend_from_slice(&supplement);
        }
        debug_assert_eq!(secret_key.len(), self.level.secret_key_len());

        Ok(SphincsPlusKeyPair {
            public_key,
            secret_key,
            level: self.level,
        })
    }

    /// Sign arbitrary data with the deterministic engine.
    pub fn sign(&self, secret_key: &[u8], message: &[u8]) -> SphincsPlusResult<Vec<u8>> {
        if secret_key.len() != self.level.secret_key_len() {
            return Err(SphincsPlusError::InvalidInput(
                "sphincs secret key length mismatch",
            ));
        }
        if secret_key.len() < DETERMINISTIC_SEED_LEN {
            return Err(SphincsPlusError::InvalidInput(
                "sphincs secret key seed is truncated",
            ));
        }
        let seed_material = &secret_key[..DETERMINISTIC_SEED_LEN];
        let derived_pk = expand_bytes(DOMAIN_PK, seed_material, self.level.public_key_len());
        Ok(self.expand_signature(&derived_pk, message))
    }

    /// Verify a deterministic signature using the advertised public key.
    pub fn verify(
        &self,
        public_key: &[u8],
        message: &[u8],
        signature: &[u8],
    ) -> SphincsPlusResult<()> {
        if public_key.len() != self.level.public_key_len() {
            return Err(SphincsPlusError::InvalidInput(
                "sphincs public key length mismatch",
            ));
        }
        if signature.len() != self.level.signature_len() {
            return Err(SphincsPlusError::InvalidInput(
                "sphincs signature length mismatch",
            ));
        }

        let expected = self.expand_signature(public_key, message);
        if expected == signature {
            Ok(())
        } else {
            Err(SphincsPlusError::VerifyFailed)
        }
    }

    fn expand_signature(&self, public_key: &[u8], message: &[u8]) -> Vec<u8> {
        let mut transcript =
            Vec::with_capacity(2 + public_key.len() + message.len() + DOMAIN_SIG.len());
        transcript.extend_from_slice(DOMAIN_SIG);
        transcript.push(self.level.domain_tag());
        transcript.extend_from_slice(public_key);
        transcript.extend_from_slice(message);
        expand_bytes(DOMAIN_SIG, &transcript, self.level.signature_len())
    }

    fn next_seed(&self) -> Vec<u8> {
        let mut guard = self.counter.lock();
        let counter = *guard;
        *guard = counter.wrapping_add(1);
        drop(guard);

        expand_bytes(DOMAIN_SEED, &counter.to_le_bytes(), DETERMINISTIC_SEED_LEN)
    }
}

impl Default for SphincsPlusDeterministic {
    fn default() -> Self {
        Self::new(SphincsPlusSecurityLevel::Shake128s)
    }
}

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
        let block: Output<Blake2s256> = digest.finalize();
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
    fn keypair_lengths_match_level() {
        let engine = SphincsPlusDeterministic::new(SphincsPlusSecurityLevel::Shake256s);
        let pair = engine.keypair().expect("keypair");
        assert_eq!(pair.public_key.len(), 64);
        assert_eq!(pair.secret_key.len(), 128);
        assert_eq!(pair.level, SphincsPlusSecurityLevel::Shake256s);
    }

    #[test]
    fn sign_and_verify_round_trip() {
        let engine = SphincsPlusDeterministic::new(SphincsPlusSecurityLevel::Shake128s);
        let pair = engine.keypair().expect("keypair");
        let msg = b"deterministic sphincs payload";
        let sig = engine.sign(&pair.secret_key, msg).expect("sign");
        assert_eq!(
            sig.len(),
            SphincsPlusSecurityLevel::Shake128s.signature_len()
        );
        engine.verify(&pair.public_key, msg, &sig).expect("verify");
    }

    #[test]
    fn verify_detects_tampering() {
        let engine = SphincsPlusDeterministic::new(SphincsPlusSecurityLevel::Shake192f);
        let pair = engine.keypair().expect("keypair");
        let msg = b"tamper me";
        let mut sig = engine.sign(&pair.secret_key, msg).expect("sign");
        sig[0] ^= 0xAA;
        let err = engine.verify(&pair.public_key, msg, &sig).unwrap_err();
        assert_eq!(err, SphincsPlusError::VerifyFailed);
    }
}
