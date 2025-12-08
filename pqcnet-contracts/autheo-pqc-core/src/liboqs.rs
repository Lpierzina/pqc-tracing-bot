use crate::dsa::{MlDsa, MlDsaEngine, MlDsaKeyPair};
use crate::error::{PqcError, PqcResult};
use crate::fallback::{FallbackDsa, FallbackKem, FallbackSwitch};
use crate::kem::{MlKem, MlKemEncapsulation, MlKemEngine, MlKemKeyPair};
use crate::key_manager::{KemKeyState, KemRotation, KeyManager, ThresholdPolicy};
use crate::secret_sharing::{
    combine_secret, split_secret, RecoveredSecret, SecretShare, SecretSharePackage,
};
use crate::signatures::{DsaKeyState, SignatureManager};
use crate::types::{Bytes, SecurityLevel, TimestampMs};
use alloc::boxed::Box;
use autheo_pqcnet_hqc::{HqcAlgorithm, HqcError, HqcLevel, HqcLibOqs};
use autheo_pqcnet_sphincs::{SphincsPlusError, SphincsPlusLibOqs, SphincsPlusSecurityLevel};
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
    pub hqc_backup: Option<HqcFallbackConfig>,
    pub sphincs_backup: Option<SphincsFallbackConfig>,
}

impl Default for LibOqsConfig {
    fn default() -> Self {
        Self {
            kem_algorithm: LibOqsKemAlgorithm::MlKem768,
            dsa_algorithm: LibOqsDsaAlgorithm::MlDsa65,
            threshold: ThresholdPolicy { t: 3, n: 5 },
            rotation_interval_ms: 300_000,
            hqc_backup: Some(HqcFallbackConfig::default()),
            sphincs_backup: Some(SphincsFallbackConfig::default()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct HqcFallbackConfig {
    pub level: HqcLevel,
    pub auto_failover: bool,
}

impl Default for HqcFallbackConfig {
    fn default() -> Self {
        Self {
            level: HqcLevel::Hqc256,
            auto_failover: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SphincsFallbackConfig {
    pub level: SphincsPlusSecurityLevel,
    pub auto_failover: bool,
}

impl Default for SphincsFallbackConfig {
    fn default() -> Self {
        Self {
            level: SphincsPlusSecurityLevel::Shake256s,
            auto_failover: true,
        }
    }
}

/// High-level wrapper that wires liboqs ML-KEM + ML-DSA into the existing managers.
pub struct LibOqsProvider {
    key_manager: KeyManager,
    signature_manager: SignatureManager,
    signing_key: Option<ActiveSigningKey>,
    config: LibOqsConfig,
    kem_switch: Option<FallbackSwitch>,
    dsa_switch: Option<FallbackSwitch>,
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

        let (kem_box, kem_switch) = match &config.hqc_backup {
            Some(backup) => {
                let primary: Box<dyn MlKem> = Box::new(LibOqsKemImpl::new(config.kem_algorithm));
                let backup_engine: Box<dyn MlKem> = Box::new(HqcKemAdapter::new(backup.level));
                let (fallback, switch) =
                    FallbackKem::new(primary, backup_engine, backup.auto_failover);
                (Box::new(fallback) as Box<dyn MlKem>, Some(switch))
            }
            None => (
                Box::new(LibOqsKemImpl::new(config.kem_algorithm)) as Box<dyn MlKem>,
                None,
            ),
        };

        let (dsa_box, dsa_switch) = match &config.sphincs_backup {
            Some(backup) => {
                let primary: Box<dyn MlDsa> = Box::new(LibOqsDsaImpl::new(config.dsa_algorithm));
                let backup_engine: Box<dyn MlDsa> = Box::new(SphincsDsaAdapter::new(backup.level));
                let (fallback, switch) =
                    FallbackDsa::new(primary, backup_engine, backup.auto_failover);
                (Box::new(fallback) as Box<dyn MlDsa>, Some(switch))
            }
            None => (
                Box::new(LibOqsDsaImpl::new(config.dsa_algorithm)) as Box<dyn MlDsa>,
                None,
            ),
        };

        let key_manager = KeyManager::new(
            MlKemEngine::new(kem_box),
            config.threshold,
            config.rotation_interval_ms,
        );
        let signature_manager = SignatureManager::new(MlDsaEngine::new(dsa_box));

        Ok(Self {
            key_manager,
            signature_manager,
            signing_key: None,
            config,
            kem_switch,
            dsa_switch,
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

    /// Force HQC backup usage for all future ML-KEM operations.
    pub fn force_hqc_backup(&self) {
        if let Some(switch) = &self.kem_switch {
            switch.force_backup();
        }
    }

    /// Return to the Kyber primary ML-KEM engine.
    pub fn use_kyber_primary(&self) {
        if let Some(switch) = &self.kem_switch {
            switch.use_primary();
        }
    }

    /// Report whether HQC backup mode is active.
    pub fn is_using_hqc_backup(&self) -> bool {
        self.kem_switch
            .as_ref()
            .map(|s| s.is_forcing_backup())
            .unwrap_or(false)
    }

    /// Force SPHINCS+ backup usage for ML-DSA operations.
    pub fn force_sphincs_backup(&self) {
        if let Some(switch) = &self.dsa_switch {
            switch.force_backup();
        }
    }

    /// Return to the Dilithium primary ML-DSA engine.
    pub fn use_dilithium_primary(&self) {
        if let Some(switch) = &self.dsa_switch {
            switch.use_primary();
        }
    }

    /// Report whether SPHINCS+ backup mode is active.
    pub fn is_using_sphincs_backup(&self) -> bool {
        self.dsa_switch
            .as_ref()
            .map(|s| s.is_forcing_backup())
            .unwrap_or(false)
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

struct HqcKemAdapter {
    engine: HqcLibOqs,
    level: HqcLevel,
}

impl HqcKemAdapter {
    fn new(level: HqcLevel) -> Self {
        let algorithm = hqc_algorithm(level);
        Self {
            engine: HqcLibOqs::new(algorithm),
            level,
        }
    }
}

impl MlKem for HqcKemAdapter {
    fn level(&self) -> SecurityLevel {
        map_hqc_level(self.level)
    }

    fn keygen(&self) -> PqcResult<MlKemKeyPair> {
        let pair = self
            .engine
            .keypair()
            .map_err(|err| map_hqc_error("hqc::keypair", err))?;
        Ok(MlKemKeyPair {
            public_key: pair.public_key,
            secret_key: pair.secret_key,
            level: map_hqc_level(pair.level),
        })
    }

    fn encapsulate(&self, public_key: &[u8]) -> PqcResult<MlKemEncapsulation> {
        let encapsulation = self
            .engine
            .encapsulate(public_key)
            .map_err(|err| map_hqc_error("hqc::encapsulate", err))?;
        Ok(MlKemEncapsulation {
            ciphertext: encapsulation.ciphertext,
            shared_secret: encapsulation.shared_secret,
        })
    }

    fn decapsulate(&self, secret_key: &[u8], ciphertext: &[u8]) -> PqcResult<Bytes> {
        self.engine
            .decapsulate(secret_key, ciphertext)
            .map_err(|err| map_hqc_error("hqc::decapsulate", err))
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

struct SphincsDsaAdapter {
    engine: SphincsPlusLibOqs,
    level: SphincsPlusSecurityLevel,
}

impl SphincsDsaAdapter {
    fn new(level: SphincsPlusSecurityLevel) -> Self {
        Self {
            engine: SphincsPlusLibOqs::new(level),
            level,
        }
    }
}

impl MlDsa for SphincsDsaAdapter {
    fn level(&self) -> SecurityLevel {
        map_sphincs_level(self.level)
    }

    fn keygen(&self) -> PqcResult<MlDsaKeyPair> {
        let pair = self
            .engine
            .keypair()
            .map_err(|err| map_sphincs_error("sphincs::keypair", err))?;
        Ok(MlDsaKeyPair {
            public_key: pair.public_key,
            secret_key: pair.secret_key,
            level: map_sphincs_level(pair.level),
        })
    }

    fn sign(&self, sk: &[u8], message: &[u8]) -> PqcResult<Bytes> {
        self.engine
            .sign(sk, message)
            .map_err(|err| map_sphincs_error("sphincs::sign", err))
    }

    fn verify(&self, pk: &[u8], message: &[u8], signature: &[u8]) -> PqcResult<()> {
        self.engine
            .verify(pk, message, signature)
            .map_err(|err| map_sphincs_error("sphincs::verify", err))
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

fn hqc_algorithm(level: HqcLevel) -> HqcAlgorithm {
    match level {
        HqcLevel::Hqc128 => HqcAlgorithm::Hqc128,
        HqcLevel::Hqc192 => HqcAlgorithm::Hqc192,
        HqcLevel::Hqc256 => HqcAlgorithm::Hqc256,
    }
}

fn map_hqc_level(level: HqcLevel) -> SecurityLevel {
    match level {
        HqcLevel::Hqc128 => SecurityLevel::MlKem128,
        HqcLevel::Hqc192 => SecurityLevel::MlKem192,
        HqcLevel::Hqc256 => SecurityLevel::MlKem256,
    }
}

fn map_hqc_error(context: &'static str, err: HqcError) -> PqcError {
    match err {
        HqcError::InvalidInput(msg) => PqcError::InvalidInput(msg),
        HqcError::IntegrationError(ctx, detail) => {
            PqcError::IntegrationError(format!("{context}:{ctx}: {detail}"))
        }
    }
}

fn map_sphincs_level(level: SphincsPlusSecurityLevel) -> SecurityLevel {
    match level {
        SphincsPlusSecurityLevel::Shake128s | SphincsPlusSecurityLevel::Shake128f => {
            SecurityLevel::MlDsa128
        }
        SphincsPlusSecurityLevel::Shake192s | SphincsPlusSecurityLevel::Shake192f => {
            SecurityLevel::MlDsa192
        }
        SphincsPlusSecurityLevel::Shake256s | SphincsPlusSecurityLevel::Shake256f => {
            SecurityLevel::MlDsa256
        }
    }
}

fn map_sphincs_error(context: &'static str, err: SphincsPlusError) -> PqcError {
    match err {
        SphincsPlusError::InvalidInput(msg) => PqcError::InvalidInput(msg),
        SphincsPlusError::VerifyFailed => PqcError::VerifyFailed,
        SphincsPlusError::IntegrationError(ctx, detail) => {
            PqcError::IntegrationError(format!("{context}:{ctx}: {detail}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SecurityLevel;

    #[test]
    fn kyber_failover_switches_to_hqc_backup() {
        let mut config = LibOqsConfig::default();
        config.sphincs_backup = None;
        config.hqc_backup = Some(HqcFallbackConfig {
            level: HqcLevel::Hqc256,
            auto_failover: false,
        });
        let mut provider = LibOqsProvider::new(config).expect("provider");

        let primary = provider.keygen(1_700_000_000_000).expect("keygen");
        assert_eq!(primary.kem_keypair.level, SecurityLevel::MlKem192);
        assert!(!provider.is_using_hqc_backup());

        provider.force_hqc_backup();
        assert!(provider.is_using_hqc_backup());

        let backup = provider.keygen(1_700_000_050_000).expect("backup keygen");
        assert_eq!(backup.kem_keypair.level, SecurityLevel::MlKem256);

        provider.use_kyber_primary();
        assert!(!provider.is_using_hqc_backup());
    }

    #[test]
    fn dilithium_failover_switches_to_sphincs_backup() {
        let mut config = LibOqsConfig::default();
        config.hqc_backup = None;
        config.sphincs_backup = Some(SphincsFallbackConfig {
            level: SphincsPlusSecurityLevel::Shake256s,
            auto_failover: false,
        });
        let mut provider = LibOqsProvider::new(config).expect("provider");

        provider.force_sphincs_backup();
        assert!(provider.is_using_sphincs_backup());

        let artifacts = provider.keygen(1_700_000_100_000).expect("keygen");
        assert_eq!(artifacts.signing_keypair.level, SecurityLevel::MlDsa256);

        let message = b"fallback signature";
        let signature = provider.sign(message).expect("sign");
        provider.verify(message, &signature).expect("verify");

        provider.use_dilithium_primary();
        assert!(!provider.is_using_sphincs_backup());
    }
}
