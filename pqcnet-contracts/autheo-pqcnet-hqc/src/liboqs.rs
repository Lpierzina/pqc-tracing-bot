#![cfg(feature = "liboqs")]

use crate::types::{HqcEncapsulation, HqcError, HqcKeyPair, HqcLevel, HqcResult};
use alloc::vec::Vec;
use oqs::kem;
use std::sync::Once;

/// HQC algorithms exposed by liboqs.
#[derive(Clone, Copy, Debug)]
pub enum HqcAlgorithm {
    Hqc128,
    Hqc192,
    Hqc256,
}

impl HqcAlgorithm {
    fn as_oqs(self) -> kem::Algorithm {
        match self {
            Self::Hqc128 => kem::Algorithm::Hqc128,
            Self::Hqc192 => kem::Algorithm::Hqc192,
            Self::Hqc256 => kem::Algorithm::Hqc256,
        }
    }

    fn level(self) -> HqcLevel {
        match self {
            Self::Hqc128 => HqcLevel::Hqc128,
            Self::Hqc192 => HqcLevel::Hqc192,
            Self::Hqc256 => HqcLevel::Hqc256,
        }
    }
}

/// liboqs-backed HQC engine.
pub struct HqcLibOqs {
    algorithm: HqcAlgorithm,
}

impl HqcLibOqs {
    /// Create the engine for the given HQC parameter set.
    pub fn new(algorithm: HqcAlgorithm) -> Self {
        ensure_liboqs_init();
        Self { algorithm }
    }

    /// Return the configured HQC level.
    pub fn level(&self) -> HqcLevel {
        self.algorithm.level()
    }

    fn instantiate(&self) -> HqcResult<kem::Kem> {
        kem::Kem::new(self.algorithm.as_oqs()).map_err(|err| map_oqs_error("kem::new", err))
    }

    /// Generate a HQC keypair through liboqs.
    pub fn keypair(&self) -> HqcResult<HqcKeyPair> {
        let kem = self.instantiate()?;
        let (public_key, secret_key) = kem
            .keypair()
            .map_err(|err| map_oqs_error("kem::keypair", err))?;

        Ok(HqcKeyPair {
            public_key: public_key.into_vec(),
            secret_key: secret_key.into_vec(),
            level: self.level(),
        })
    }

    /// Encapsulate to the provided HQC public key.
    pub fn encapsulate(&self, public_key: &[u8]) -> HqcResult<HqcEncapsulation> {
        let kem = self.instantiate()?;
        let pk = kem
            .public_key_from_bytes(public_key)
            .ok_or(HqcError::InvalidInput("hqc public key length mismatch"))?;
        let (ciphertext, shared_secret) = kem
            .encapsulate(pk)
            .map_err(|err| map_oqs_error("kem::encapsulate", err))?;

        Ok(HqcEncapsulation {
            ciphertext: ciphertext.into_vec(),
            shared_secret: shared_secret.into_vec(),
        })
    }

    /// Decapsulate a HQC ciphertext with the provided secret key.
    pub fn decapsulate(&self, secret_key: &[u8], ciphertext: &[u8]) -> HqcResult<Vec<u8>> {
        let kem = self.instantiate()?;
        let sk = kem
            .secret_key_from_bytes(secret_key)
            .ok_or(HqcError::InvalidInput("hqc secret key length mismatch"))?;
        let ct = kem
            .ciphertext_from_bytes(ciphertext)
            .ok_or(HqcError::InvalidInput("hqc ciphertext length mismatch"))?;

        let shared_secret = kem
            .decapsulate(sk, ct)
            .map_err(|err| map_oqs_error("kem::decapsulate", err))?;
        Ok(shared_secret.into_vec())
    }
}

fn ensure_liboqs_init() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        oqs::init();
    });
}

fn map_oqs_error(context: &'static str, err: oqs::Error) -> HqcError {
    HqcError::integration(context, err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liboqs_keygen_and_roundtrip() {
        let engine = HqcLibOqs::new(HqcAlgorithm::Hqc192);
        let pair = engine.keypair().expect("keypair");
        assert_eq!(pair.level, HqcLevel::Hqc192);
        assert!(!pair.public_key.is_empty());
        assert!(!pair.secret_key.is_empty());

        let enc = engine.encapsulate(&pair.public_key).expect("encapsulate");
        let shared = engine
            .decapsulate(&pair.secret_key, &enc.ciphertext)
            .expect("decapsulate");
        assert_eq!(shared, enc.shared_secret);
    }
}
