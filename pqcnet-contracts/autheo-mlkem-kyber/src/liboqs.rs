#![cfg(all(feature = "liboqs", not(target_arch = "wasm32")))]

use crate::types::{KyberEncapsulation, KyberError, KyberKeyPair, KyberLevel, KyberResult};
use alloc::vec::Vec;
use oqs::kem;
use std::sync::Once;

/// Supported Kyber algorithms exposed by liboqs.
#[derive(Clone, Copy, Debug)]
pub enum KyberAlgorithm {
    MlKem512,
    MlKem768,
    MlKem1024,
}

impl KyberAlgorithm {
    fn as_oqs(self) -> kem::Algorithm {
        match self {
            Self::MlKem512 => kem::Algorithm::Kyber512,
            Self::MlKem768 => kem::Algorithm::Kyber768,
            Self::MlKem1024 => kem::Algorithm::Kyber1024,
        }
    }

    fn level(self) -> KyberLevel {
        match self {
            Self::MlKem512 => KyberLevel::MlKem512,
            Self::MlKem768 => KyberLevel::MlKem768,
            Self::MlKem1024 => KyberLevel::MlKem1024,
        }
    }
}

/// liboqs-backed Kyber engine.
pub struct KyberLibOqs {
    algorithm: KyberAlgorithm,
}

impl KyberLibOqs {
    /// Create a new liboqs-backed engine for the requested algorithm.
    pub fn new(algorithm: KyberAlgorithm) -> Self {
        ensure_liboqs_init();
        Self { algorithm }
    }

    /// Return the configured Kyber level.
    pub fn level(&self) -> KyberLevel {
        self.algorithm.level()
    }

    fn instantiate(&self) -> KyberResult<kem::Kem> {
        kem::Kem::new(self.algorithm.as_oqs()).map_err(|err| map_oqs_error("kem::new", err))
    }

    /// Generate a Kyber key pair backed by liboqs.
    pub fn keypair(&self) -> KyberResult<KyberKeyPair> {
        let kem = self.instantiate()?;
        let (public_key, secret_key) = kem
            .keypair()
            .map_err(|err| map_oqs_error("kem::keypair", err))?;

        Ok(KyberKeyPair {
            public_key: public_key.into_vec(),
            secret_key: secret_key.into_vec(),
            level: self.level(),
        })
    }

    /// Encapsulate to the provided public key.
    pub fn encapsulate(&self, public_key: &[u8]) -> KyberResult<KyberEncapsulation> {
        let kem = self.instantiate()?;
        let pk = kem
            .public_key_from_bytes(public_key)
            .ok_or(KyberError::InvalidInput(
                "ml-kem public key length mismatch",
            ))?;
        let (ciphertext, shared_secret) = kem
            .encapsulate(pk)
            .map_err(|err| map_oqs_error("kem::encapsulate", err))?;

        Ok(KyberEncapsulation {
            ciphertext: ciphertext.into_vec(),
            shared_secret: shared_secret.into_vec(),
        })
    }

    /// Decapsulate the ciphertext with the provided secret key.
    pub fn decapsulate(&self, secret_key: &[u8], ciphertext: &[u8]) -> KyberResult<Vec<u8>> {
        let kem = self.instantiate()?;
        let sk = kem
            .secret_key_from_bytes(secret_key)
            .ok_or(KyberError::InvalidInput(
                "ml-kem secret key length mismatch",
            ))?;
        let ct = kem
            .ciphertext_from_bytes(ciphertext)
            .ok_or(KyberError::InvalidInput(
                "ml-kem ciphertext length mismatch",
            ))?;

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

fn map_oqs_error(context: &'static str, err: oqs::Error) -> KyberError {
    KyberError::integration(context, err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liboqs_keygen_and_encapsulate_round_trip() {
        let engine = KyberLibOqs::new(KyberAlgorithm::MlKem768);
        let pair = engine.keypair().expect("keypair");
        assert_eq!(pair.level, KyberLevel::MlKem768);
        assert!(!pair.public_key.is_empty());
        assert!(!pair.secret_key.is_empty());

        let enc = engine.encapsulate(&pair.public_key).expect("encapsulate");
        let shared = engine
            .decapsulate(&pair.secret_key, &enc.ciphertext)
            .expect("decapsulate");
        assert_eq!(shared, enc.shared_secret);
    }
}
