use crate::dsa::{MlDsa, MlDsaEngine, MlDsaKeyPair};
use crate::error::{PqcError, PqcResult};
use crate::kem::{MlKem, MlKemEncapsulation, MlKemEngine, MlKemKeyPair};
use crate::key_manager::{KemKeyState, KemRotation, KeyManager, ThresholdPolicy};
use crate::secret_sharing::{
    combine_secret, split_secret, RecoveredSecret, SecretShare, SecretSharePackage,
};
use crate::signatures::{DsaKeyState, SignatureManager};
use crate::types::{Bytes, SecurityLevel, TimestampMs};
use alloc::boxed::Box;
use oqs::{kem, sig};
use std::sync::Once;

/// Supported ML-KEM profiles exposed by liboqs.
#[derive(Clone, Copy, Debug)]
pub enum LibOqsKemAlgorithm {
    MlKem512,
    MlKem768,
    MlKem1024,
}

impl LibOqsKemAlgorithm {
    fn as_oqs(self) -> kem::Algorithm {
        match self {
            Self::MlKem512 => kem::Algorithm::Kyber512,
            Self::MlKem768 => kem::Algorithm::Kyber768,
            Self::MlKem1024 => kem::Algorithm::Kyber1024,
        }
    }

    fn level(self) -> SecurityLevel {
        match self {
            Self::MlKem512 => SecurityLevel::MlKem128,
            Self::MlKem768 => SecurityLevel::MlKem192,
            Self::MlKem1024 => SecurityLevel::MlKem256,
        }
    }
}

/// Supported ML-DSA (Dilithium) profiles exposed by liboqs.
#[derive(Clone, Copy, Debug)]
pub enum LibOqsDsaAlgorithm {
    MlDsa44,
    MlDsa65,
    MlDsa87,
}

impl LibOqsDsaAlgorithm {
    fn as_oqs(self) -> sig::Algorithm {
        match self {
            Self::MlDsa44 => sig::Algorithm::Dilithium2,
            Self::MlDsa65 => sig::Algorithm::Dilithium3,
            Self::MlDsa87 => sig::Algorithm::Dilithium5,
        }
    }

    fn level(self) -> SecurityLevel {
        match self {
            Self::MlDsa44 => SecurityLevel::MlDsa128,
            Self::MlDsa65 => SecurityLevel::MlDsa192,
            Self::MlDsa87 => SecurityLevel::MlDsa256,
        }
    }
}

/// Configuration for the liboqs-backed PQC wrapper.
#[derive(Clone, Debug)]
pub struct LibOqsConfig {
    pub kem_algorithm: LibOqsKemAlgorithm,
    pub dsa_algorithm: LibOqsDsaAlgorithm,
    pub threshold: ThresholdPolicy,
    pub rotation_interval_ms: u64,
}

impl Default for LibOqsConfig {
    fn default() -> Self {
        Self {
            kem_algorithm: LibOqsKemAlgorithm::MlKem768,
            dsa_algorithm: LibOqsDsaAlgorithm::MlDsa65,
            threshold: ThresholdPolicy { t: 3, n: 5 },
            rotation_interval_ms: 300_000,
        }
    }
}

/// High-level wrapper that wires liboqs ML-KEM + ML-DSA into the existing managers.
pub struct LibOqsProvider {
    key_manager: KeyManager,
    signature_manager: SignatureManager,
    signing_key: Option<ActiveSigningKey>,
    config: LibOqsConfig,
}

struct ActiveSigningKey {
    state: DsaKeyState,
    pair: MlDsaKeyPair,
}

impl ActiveSigningKey {
    fn new(state: DsaKeyState, pair: MlDsaKeyPair) -> Self {
        Self { state, pair }
    }
}

/// Combined key material returned by [`LibOqsProvider::keygen`].
#[derive(Clone)]
pub struct KeygenArtifacts {
    pub kem_state: KemKeyState,
    pub kem_keypair: MlKemKeyPair,
    pub kem_shares: SecretSharePackage,
    pub signing_state: DsaKeyState,
    pub signing_keypair: MlDsaKeyPair,
}

/// Rotation output returned by [`LibOqsProvider::rotate`].
#[derive(Clone)]
pub struct RotationArtifacts {
    pub kem: KemRotation,
    pub kem_shares: SecretSharePackage,
    pub signing_state: DsaKeyState,
    pub signing_keypair: MlDsaKeyPair,
}

impl LibOqsProvider {
    /// Create a new liboqs-backed PQC provider.
    pub fn new(config: LibOqsConfig) -> PqcResult<Self> {
        ensure_liboqs_init();

        let kem = Box::new(LibOqsKemImpl::new(config.kem_algorithm));
        let dsa = Box::new(LibOqsDsaImpl::new(config.dsa_algorithm));

        let key_manager = KeyManager::new(
            MlKemEngine::new(kem),
            config.threshold,
            config.rotation_interval_ms,
        );
        let signature_manager = SignatureManager::new(MlDsaEngine::new(dsa));

        Ok(Self {
            key_manager,
            signature_manager,
            signing_key: None,
            config,
        })
    }

    /// Generate ML-KEM + ML-DSA key material and record the active signing key.
    pub fn keygen(&mut self, now_ms: TimestampMs) -> PqcResult<KeygenArtifacts> {
        let (kem_state, kem_pair) = self.key_manager.keygen_with_material(now_ms)?;
        let kem_shares = split_secret(
            &kem_pair.secret_key,
            &kem_state.id,
            kem_state.version,
            kem_state.created_at,
            self.key_manager.threshold_policy(),
        )?;
        let (signing_state, signing_pair) = self.signature_manager.generate_signing_key(now_ms)?;
        self.signing_key = Some(ActiveSigningKey::new(
            signing_state.clone(),
            signing_pair.clone(),
        ));

        Ok(KeygenArtifacts {
            kem_state,
            kem_keypair: kem_pair,
            kem_shares,
            signing_state,
            signing_keypair: signing_pair,
        })
    }

    /// Rotate the ML-KEM key if expired and refresh the signing key.
    pub fn rotate(&mut self, now_ms: TimestampMs) -> PqcResult<Option<RotationArtifacts>> {
        match self.key_manager.rotate_with_material(now_ms)? {
            Some(kem_rotation) => {
                let kem_shares = split_secret(
                    &kem_rotation.new_material.secret_key,
                    &kem_rotation.new.id,
                    kem_rotation.new.version,
                    kem_rotation.new.created_at,
                    self.key_manager.threshold_policy(),
                )?;
                let (signing_state, signing_pair) =
                    self.signature_manager.generate_signing_key(now_ms)?;
                self.signing_key = Some(ActiveSigningKey::new(
                    signing_state.clone(),
                    signing_pair.clone(),
                ));
                Ok(Some(RotationArtifacts {
                    kem: kem_rotation,
                    kem_shares,
                    signing_state,
                    signing_keypair: signing_pair,
                }))
            }
            None => Ok(None),
        }
    }

    /// Sign arbitrary data using the current ML-DSA key.
    pub fn sign(&self, data: &[u8]) -> PqcResult<Bytes> {
        let key = self
            .signing_key
            .as_ref()
            .ok_or(PqcError::InternalError("signing key not initialized"))?;
        self.signature_manager.sign(&key.pair.secret_key, data)
    }

    /// Verify a signature with the active ML-DSA public key.
    pub fn verify(&self, data: &[u8], signature: &[u8]) -> PqcResult<()> {
        let key = self
            .signing_key
            .as_ref()
            .ok_or(PqcError::InternalError("signing key not initialized"))?;
        self.signature_manager
            .verify(&key.state.id, data, signature)
    }

    /// Encapsulate to the current ML-KEM key.
    pub fn encapsulate_for_current(&self) -> PqcResult<(KemKeyState, MlKemEncapsulation)> {
        self.key_manager.encapsulate_for_current()
    }

    /// Expose the configured threshold policy.
    pub fn threshold(&self) -> ThresholdPolicy {
        self.config.threshold
    }

    /// Reconstruct a ML-KEM secret key from a quorum of shares.
    pub fn combine_kem_secret(&self, shares: &[SecretShare]) -> PqcResult<RecoveredSecret> {
        combine_secret(shares)
    }
}

struct LibOqsKemImpl {
    algorithm: LibOqsKemAlgorithm,
}

impl LibOqsKemImpl {
    fn new(algorithm: LibOqsKemAlgorithm) -> Self {
        Self { algorithm }
    }

    fn instantiate(&self) -> PqcResult<kem::Kem> {
        kem::Kem::new(self.algorithm.as_oqs()).map_err(|err| map_oqs_error("kem::new", err))
    }
}

impl MlKem for LibOqsKemImpl {
    fn level(&self) -> SecurityLevel {
        self.algorithm.level()
    }

    fn keygen(&self) -> PqcResult<MlKemKeyPair> {
        let kem = self.instantiate()?;
        let (public_key, secret_key) = kem
            .keypair()
            .map_err(|err| map_oqs_error("kem::keypair", err))?;
        Ok(MlKemKeyPair {
            public_key: public_key.into_vec(),
            secret_key: secret_key.into_vec(),
            level: self.level(),
        })
    }

    fn encapsulate(&self, public_key: &[u8]) -> PqcResult<MlKemEncapsulation> {
        let kem = self.instantiate()?;
        let pk_ref = kem
            .public_key_from_bytes(public_key)
            .ok_or(PqcError::InvalidInput("ml-kem public key length mismatch"))?;
        let (ciphertext, shared_secret) = kem
            .encapsulate(pk_ref)
            .map_err(|err| map_oqs_error("kem::encapsulate", err))?;
        Ok(MlKemEncapsulation {
            ciphertext: ciphertext.into_vec(),
            shared_secret: shared_secret.into_vec(),
        })
    }

    fn decapsulate(&self, secret_key: &[u8], ciphertext: &[u8]) -> PqcResult<Bytes> {
        let kem = self.instantiate()?;
        let sk_ref = kem
            .secret_key_from_bytes(secret_key)
            .ok_or(PqcError::InvalidInput("ml-kem secret key length mismatch"))?;
        let ct_ref = kem
            .ciphertext_from_bytes(ciphertext)
            .ok_or(PqcError::InvalidInput("ml-kem ciphertext length mismatch"))?;
        let shared_secret = kem
            .decapsulate(sk_ref, ct_ref)
            .map_err(|err| map_oqs_error("kem::decapsulate", err))?;
        Ok(shared_secret.into_vec())
    }
}

struct LibOqsDsaImpl {
    algorithm: LibOqsDsaAlgorithm,
}

impl LibOqsDsaImpl {
    fn new(algorithm: LibOqsDsaAlgorithm) -> Self {
        Self { algorithm }
    }

    fn instantiate(&self) -> PqcResult<sig::Sig> {
        sig::Sig::new(self.algorithm.as_oqs()).map_err(|err| map_oqs_error("sig::new", err))
    }
}

impl MlDsa for LibOqsDsaImpl {
    fn level(&self) -> SecurityLevel {
        self.algorithm.level()
    }

    fn keygen(&self) -> PqcResult<MlDsaKeyPair> {
        let sig = self.instantiate()?;
        let (public_key, secret_key) = sig
            .keypair()
            .map_err(|err| map_oqs_error("sig::keypair", err))?;
        Ok(MlDsaKeyPair {
            public_key: public_key.into_vec(),
            secret_key: secret_key.into_vec(),
            level: self.level(),
        })
    }

    fn sign(&self, sk: &[u8], message: &[u8]) -> PqcResult<Bytes> {
        let sig = self.instantiate()?;
        let sk_ref = sig
            .secret_key_from_bytes(sk)
            .ok_or(PqcError::InvalidInput("ml-dsa secret key length mismatch"))?;
        let signature = sig
            .sign(message, sk_ref)
            .map_err(|err| map_oqs_error("sig::sign", err))?;
        Ok(signature.into_vec())
    }

    fn verify(&self, pk: &[u8], message: &[u8], signature: &[u8]) -> PqcResult<()> {
        let sig = self.instantiate()?;
        let pk_ref = sig
            .public_key_from_bytes(pk)
            .ok_or(PqcError::InvalidInput("ml-dsa public key length mismatch"))?;
        let sig_ref = sig
            .signature_from_bytes(signature)
            .ok_or(PqcError::InvalidInput("ml-dsa signature length mismatch"))?;
        sig.verify(message, sig_ref, pk_ref)
            .map_err(|err| map_oqs_error("sig::verify", err))
    }
}

fn ensure_liboqs_init() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        oqs::init();
    });
}

fn map_oqs_error(context: &'static str, err: oqs::Error) -> PqcError {
    PqcError::IntegrationError(format!("{context}: {err}"))
}
