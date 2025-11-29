#![cfg(all(feature = "liboqs", not(target_arch = "wasm32")))]

use crate::types::{
    DilithiumError, DilithiumKeyPair, DilithiumLevel, DilithiumResult,
};
use alloc::vec::Vec;
use oqs::sig;
use std::sync::Once;

/// Supported Dilithium algorithms exposed via liboqs.
#[derive(Clone, Copy, Debug)]
pub enum DilithiumAlgorithm {
    MlDsa44,
    MlDsa65,
    MlDsa87,
}

impl DilithiumAlgorithm {
    fn as_oqs(self) -> sig::Algorithm {
        match self {
            Self::MlDsa44 => sig::Algorithm::Dilithium2,
            Self::MlDsa65 => sig::Algorithm::Dilithium3,
            Self::MlDsa87 => sig::Algorithm::Dilithium5,
        }
    }

    fn level(self) -> DilithiumLevel {
        match self {
            Self::MlDsa44 => DilithiumLevel::MlDsa44,
            Self::MlDsa65 => DilithiumLevel::MlDsa65,
            Self::MlDsa87 => DilithiumLevel::MlDsa87,
        }
    }
}

/// liboqs-backed Dilithium engine.
pub struct DilithiumLibOqs {
    algorithm: DilithiumAlgorithm,
}

impl DilithiumLibOqs {
    /// Instantiate a liboqs-backed Dilithium engine.
    pub fn new(algorithm: DilithiumAlgorithm) -> Self {
        ensure_liboqs_init();
        Self { algorithm }
    }

    /// Report the ML-DSA security level for this engine.
    pub fn level(&self) -> DilithiumLevel {
        self.algorithm.level()
    }

    fn instantiate(&self) -> DilithiumResult<sig::Sig> {
        sig::Sig::new(self.algorithm.as_oqs()).map_err(|err| map_oqs_error("sig::new", err))
    }

    /// Generate a Dilithium keypair using liboqs.
    pub fn keypair(&self) -> DilithiumResult<DilithiumKeyPair> {
        let sig = self.instantiate()?;
        let (public_key, secret_key) = sig
            .keypair()
            .map_err(|err| map_oqs_error("sig::keypair", err))?;
        Ok(DilithiumKeyPair {
            public_key: public_key.into_vec(),
            secret_key: secret_key.into_vec(),
            level: self.level(),
        })
    }

    /// Sign a message with the provided secret key bytes.
    pub fn sign(&self, secret_key: &[u8], message: &[u8]) -> DilithiumResult<Vec<u8>> {
        let sig = self.instantiate()?;
        let sk = sig
            .secret_key_from_bytes(secret_key)
            .ok_or(DilithiumError::InvalidInput(
                "ml-dsa secret key length mismatch",
            ))?;
        let signature = sig
            .sign(message, sk)
            .map_err(|err| map_oqs_error("sig::sign", err))?;
        Ok(signature.into_vec())
    }

    /// Verify a Dilithium signature.
    pub fn verify(&self, public_key: &[u8], message: &[u8], signature: &[u8]) -> DilithiumResult<()> {
        let sig = self.instantiate()?;
        let pk = sig
            .public_key_from_bytes(public_key)
            .ok_or(DilithiumError::InvalidInput(
                "ml-dsa public key length mismatch",
            ))?;
        let sig_ref = sig
            .signature_from_bytes(signature)
            .ok_or(DilithiumError::InvalidInput(
                "ml-dsa signature length mismatch",
            ))?;
        sig.verify(message, sig_ref, pk)
            .map_err(|err| map_oqs_error("sig::verify", err))
    }
}

fn ensure_liboqs_init() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        oqs::init();
    });
}

fn map_oqs_error(context: &'static str, err: oqs::Error) -> DilithiumError {
    DilithiumError::integration(context, err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liboqs_sign_and_verify_round_trip() {
        let engine = DilithiumLibOqs::new(DilithiumAlgorithm::MlDsa65);
        let pair = engine.keypair().expect("keypair");
        assert_eq!(pair.level, DilithiumLevel::MlDsa65);
        assert!(!pair.public_key.is_empty());
        assert!(!pair.secret_key.is_empty());

        let msg = b"autheo pqc";
        let sig = engine.sign(&pair.secret_key, msg).expect("sign");
        engine
            .verify(&pair.public_key, msg, &sig)
            .expect("verify");
    }
}
