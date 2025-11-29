#![cfg(all(feature = "liboqs", not(target_arch = "wasm32")))]

use crate::types::{FalconError, FalconKeyPair, FalconLevel, FalconResult};
use alloc::vec::Vec;
use oqs::sig;
use std::sync::Once;

/// Supported Falcon algorithms exposed by liboqs.
#[derive(Clone, Copy, Debug)]
pub enum FalconAlgorithm {
    Falcon512,
    Falcon1024,
}

impl FalconAlgorithm {
    fn as_oqs(self) -> sig::Algorithm {
        match self {
            Self::Falcon512 => sig::Algorithm::Falcon512,
            Self::Falcon1024 => sig::Algorithm::Falcon1024,
        }
    }

    fn level(self) -> FalconLevel {
        match self {
            Self::Falcon512 => FalconLevel::Falcon512,
            Self::Falcon1024 => FalconLevel::Falcon1024,
        }
    }
}

/// liboqs-backed Falcon engine.
pub struct FalconLibOqs {
    algorithm: FalconAlgorithm,
}

impl FalconLibOqs {
    /// Instantiate a liboqs-backed Falcon engine.
    pub fn new(algorithm: FalconAlgorithm) -> Self {
        ensure_liboqs_init();
        Self { algorithm }
    }

    /// Report the Falcon level.
    pub fn level(&self) -> FalconLevel {
        self.algorithm.level()
    }

    fn instantiate(&self) -> FalconResult<sig::Sig> {
        sig::Sig::new(self.algorithm.as_oqs()).map_err(|err| map_oqs_error("sig::new", err))
    }

    /// Generate a Falcon keypair.
    pub fn keypair(&self) -> FalconResult<FalconKeyPair> {
        let sig = self.instantiate()?;
        let (public_key, secret_key) = sig
            .keypair()
            .map_err(|err| map_oqs_error("sig::keypair", err))?;
        Ok(FalconKeyPair {
            public_key: public_key.into_vec(),
            secret_key: secret_key.into_vec(),
            level: self.level(),
        })
    }

    /// Sign arbitrary data.
    pub fn sign(&self, secret_key: &[u8], message: &[u8]) -> FalconResult<Vec<u8>> {
        let sig = self.instantiate()?;
        let sk = sig
            .secret_key_from_bytes(secret_key)
            .ok_or(FalconError::InvalidInput(
                "falcon secret key length mismatch",
            ))?;
        let signature = sig
            .sign(message, sk)
            .map_err(|err| map_oqs_error("sig::sign", err))?;
        Ok(signature.into_vec())
    }

    /// Verify a Falcon signature.
    pub fn verify(&self, public_key: &[u8], message: &[u8], signature: &[u8]) -> FalconResult<()> {
        let sig = self.instantiate()?;
        let pk = sig
            .public_key_from_bytes(public_key)
            .ok_or(FalconError::InvalidInput(
                "falcon public key length mismatch",
            ))?;
        let sig_ref = sig
            .signature_from_bytes(signature)
            .ok_or(FalconError::InvalidInput(
                "falcon signature length mismatch",
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

fn map_oqs_error(context: &'static str, err: oqs::Error) -> FalconError {
    FalconError::integration(context, err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liboqs_sign_verify_round_trip() {
        let engine = FalconLibOqs::new(FalconAlgorithm::Falcon1024);
        let pair = engine.keypair().expect("keypair");
        assert_eq!(pair.level, FalconLevel::Falcon1024);
        assert!(!pair.public_key.is_empty());
        assert!(!pair.secret_key.is_empty());

        let msg = b"falcon liboqs";
        let sig = engine.sign(&pair.secret_key, msg).expect("sign");
        engine.verify(&pair.public_key, msg, &sig).expect("verify");
    }
}
