use crate::dsa::{MlDsa, MlDsaKeyPair};
use crate::error::{PqcError, PqcResult};
use crate::kem::{MlKem, MlKemEncapsulation, MlKemKeyPair};
use crate::types::SecurityLevel;
use alloc::vec::Vec;
use autheo_mldsa_dilithium::{
    DilithiumDeterministic, DilithiumError, DilithiumKeyPair, DilithiumLevel,
};
use autheo_mldsa_falcon::{FalconDeterministic, FalconError, FalconKeyPair, FalconLevel};
use autheo_mlkem_kyber::{
    KyberDeterministic, KyberEncapsulation, KyberError, KyberKeyPair, KyberLevel,
};

/// Deterministic Kyber adapter exported for tests/demo builds.
pub type DemoMlKem = KyberDeterministic;
/// Deterministic Dilithium adapter exported for tests/demo builds.
pub type DemoMlDsa = DilithiumDeterministic;
/// Deterministic Falcon adapter for alternative ML-DSA flows.
pub type DemoFalconDsa = FalconDeterministic;

impl From<KyberError> for PqcError {
    fn from(value: KyberError) -> Self {
        match value {
            KyberError::InvalidInput(msg) => PqcError::InvalidInput(msg),
        }
    }
}

impl From<DilithiumError> for PqcError {
    fn from(value: DilithiumError) -> Self {
        match value {
            DilithiumError::InvalidInput(msg) => PqcError::InvalidInput(msg),
            DilithiumError::VerifyFailed => PqcError::VerifyFailed,
        }
    }
}

impl From<FalconError> for PqcError {
    fn from(value: FalconError) -> Self {
        match value {
            FalconError::InvalidInput(msg) => PqcError::InvalidInput(msg),
            FalconError::VerifyFailed => PqcError::VerifyFailed,
        }
    }
}

impl MlKem for KyberDeterministic {
    fn level(&self) -> SecurityLevel {
        map_kem_level(self.level())
    }

    fn keygen(&self) -> PqcResult<MlKemKeyPair> {
        let keypair: KyberKeyPair = self.keypair().map_err(PqcError::from)?;
        Ok(MlKemKeyPair {
            public_key: keypair.public_key,
            secret_key: keypair.secret_key,
            level: map_kem_level(keypair.level),
        })
    }

    fn encapsulate(&self, public_key: &[u8]) -> PqcResult<MlKemEncapsulation> {
        let enc: KyberEncapsulation = self.encapsulate(public_key).map_err(PqcError::from)?;
        Ok(MlKemEncapsulation {
            ciphertext: enc.ciphertext,
            shared_secret: enc.shared_secret,
        })
    }

    fn decapsulate(&self, secret_key: &[u8], ciphertext: &[u8]) -> PqcResult<Vec<u8>> {
        self.decapsulate(secret_key, ciphertext)
            .map_err(PqcError::from)
    }
}

impl MlDsa for DilithiumDeterministic {
    fn level(&self) -> SecurityLevel {
        map_dilithium_level(self.level())
    }

    fn keygen(&self) -> PqcResult<MlDsaKeyPair> {
        let pair: DilithiumKeyPair = self.keypair().map_err(PqcError::from)?;
        Ok(MlDsaKeyPair {
            public_key: pair.public_key,
            secret_key: pair.secret_key,
            level: map_dilithium_level(pair.level),
        })
    }

    fn sign(&self, sk: &[u8], message: &[u8]) -> PqcResult<Vec<u8>> {
        self.sign(sk, message).map_err(PqcError::from)
    }

    fn verify(&self, pk: &[u8], message: &[u8], signature: &[u8]) -> PqcResult<()> {
        self.verify(pk, message, signature).map_err(PqcError::from)
    }
}

impl MlDsa for FalconDeterministic {
    fn level(&self) -> SecurityLevel {
        map_falcon_level(self.level())
    }

    fn keygen(&self) -> PqcResult<MlDsaKeyPair> {
        let pair: FalconKeyPair = self.keypair().map_err(PqcError::from)?;
        Ok(MlDsaKeyPair {
            public_key: pair.public_key,
            secret_key: pair.secret_key,
            level: map_falcon_level(pair.level),
        })
    }

    fn sign(&self, sk: &[u8], message: &[u8]) -> PqcResult<Vec<u8>> {
        self.sign(sk, message).map_err(PqcError::from)
    }

    fn verify(&self, pk: &[u8], message: &[u8], signature: &[u8]) -> PqcResult<()> {
        self.verify(pk, message, signature).map_err(PqcError::from)
    }
}

fn map_kem_level(level: KyberLevel) -> SecurityLevel {
    match level {
        KyberLevel::MlKem768 => SecurityLevel::MlKem192,
    }
}

fn map_dilithium_level(level: DilithiumLevel) -> SecurityLevel {
    match level {
        DilithiumLevel::MlDsa65 => SecurityLevel::MlDsa192,
    }
}

fn map_falcon_level(level: FalconLevel) -> SecurityLevel {
    match level {
        FalconLevel::Falcon1024 => SecurityLevel::MlDsa256,
    }
}
