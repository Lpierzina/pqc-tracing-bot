#![cfg(all(feature = "liboqs", not(target_arch = "wasm32")))]

use crate::types::{
    SphincsPlusError, SphincsPlusKeyPair, SphincsPlusResult, SphincsPlusSecurityLevel,
};
use alloc::vec::Vec;
use oqs::sig::{self, Algorithm};
use std::sync::Once;

/// liboqs-backed SPHINCS+ engine.
pub struct SphincsPlusLibOqs {
    level: SphincsPlusSecurityLevel,
}

impl SphincsPlusLibOqs {
    /// Instantiate a liboqs-backed SPHINCS+ engine.
    pub fn new(level: SphincsPlusSecurityLevel) -> Self {
        ensure_liboqs_init();
        Self { level }
    }

    /// Return the configured SPHINCS+ level.
    pub const fn level(&self) -> SphincsPlusSecurityLevel {
        self.level
    }

    fn instantiate(&self) -> SphincsPlusResult<sig::Sig> {
        sig::Sig::new(level_to_algorithm(self.level)).map_err(|err| map_oqs_error("sig::new", err))
    }

    /// Generate a SPHINCS+ keypair.
    pub fn keypair(&self) -> SphincsPlusResult<SphincsPlusKeyPair> {
        let sig = self.instantiate()?;
        let (public_key, secret_key) = sig
            .keypair()
            .map_err(|err| map_oqs_error("sig::keypair", err))?;

        Ok(SphincsPlusKeyPair {
            public_key: public_key.into_vec(),
            secret_key: secret_key.into_vec(),
            level: self.level,
        })
    }

    /// Sign arbitrary data with the configured algorithm.
    pub fn sign(&self, secret_key: &[u8], message: &[u8]) -> SphincsPlusResult<Vec<u8>> {
        let sig = self.instantiate()?;
        let sk = sig
            .secret_key_from_bytes(secret_key)
            .ok_or(SphincsPlusError::InvalidInput(
                "sphincs secret key length mismatch",
            ))?;
        let signature = sig
            .sign(message, sk)
            .map_err(|err| map_oqs_error("sig::sign", err))?;
        Ok(signature.into_vec())
    }

    /// Verify a SPHINCS+ signature.
    pub fn verify(
        &self,
        public_key: &[u8],
        message: &[u8],
        signature: &[u8],
    ) -> SphincsPlusResult<()> {
        let sig = self.instantiate()?;
        let pk = sig
            .public_key_from_bytes(public_key)
            .ok_or(SphincsPlusError::InvalidInput(
                "sphincs public key length mismatch",
            ))?;
        let sig_ref = sig
            .signature_from_bytes(signature)
            .ok_or(SphincsPlusError::InvalidInput(
                "sphincs signature length mismatch",
            ))?;
        sig.verify(message, sig_ref, pk)
            .map_err(|err| map_oqs_error("sig::verify", err))
    }
}

fn level_to_algorithm(level: SphincsPlusSecurityLevel) -> Algorithm {
    match level {
        SphincsPlusSecurityLevel::Shake128s => Algorithm::SphincsShake128sSimple,
        SphincsPlusSecurityLevel::Shake128f => Algorithm::SphincsShake128fSimple,
        SphincsPlusSecurityLevel::Shake192s => Algorithm::SphincsShake192sSimple,
        SphincsPlusSecurityLevel::Shake192f => Algorithm::SphincsShake192fSimple,
        SphincsPlusSecurityLevel::Shake256s => Algorithm::SphincsShake256sSimple,
        SphincsPlusSecurityLevel::Shake256f => Algorithm::SphincsShake256fSimple,
    }
}

fn ensure_liboqs_init() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        oqs::init();
    });
}

fn map_oqs_error(context: &'static str, err: oqs::Error) -> SphincsPlusError {
    SphincsPlusError::integration(context, err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liboqs_sign_verify_round_trip() {
        let engine = SphincsPlusLibOqs::new(SphincsPlusSecurityLevel::Shake128f);
        let pair = engine.keypair().expect("keypair");
        assert!(!pair.public_key.is_empty());
        assert!(!pair.secret_key.is_empty());
        let msg = b"liboqs sphincs message";
        let sig = engine.sign(&pair.secret_key, msg).expect("sign");
        engine.verify(&pair.public_key, msg, &sig).expect("verify");
    }
}
