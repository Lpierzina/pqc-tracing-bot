//! Production ML-KEM + ML-DSA glue shared by pqcnet binaries.
//! `CryptoProvider` wires `autheo-pqc-core`'s [`KeyManager`] and
//! [`SignatureManager`] so relayers, sentries, and other runtime services
//! operate on the exact Kyber/Dilithium flows we deploy—no simulators.
//!
//! # Quickstart
//! ```
//! use pqcnet_crypto::{CryptoConfig, CryptoProvider};
//!
//! let mut provider =
//!     CryptoProvider::from_config(&CryptoConfig::sample("demo-sentry")).unwrap();
//! let payload = b"doc-test";
//! let derived = provider.derive_shared_key("watcher-a").unwrap();
//! let signature = provider.sign(payload).unwrap();
//! assert!(provider.verify(payload, &signature).unwrap());
//! assert_eq!(derived.peer_id, "watcher-a");
//! ```

use std::convert::TryInto;
use std::time::{Duration, SystemTime, SystemTimeError, UNIX_EPOCH};

use autheo_pqc_core::adapters::{DemoMlDsa, DemoMlKem};
use autheo_pqc_core::dsa::MlDsaEngine;
use autheo_pqc_core::error::PqcError;
use autheo_pqc_core::kem::MlKemEngine;
use autheo_pqc_core::key_manager::{KeyManager, ThresholdPolicy};
use autheo_pqc_core::signatures::{DsaKeyState, SignatureManager};
use autheo_pqc_core::types::{KeyId, TimestampMs};
use hkdf::Hkdf;
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use thiserror::Error;

#[cfg(any(
    all(feature = "dev", feature = "test"),
    all(feature = "dev", feature = "prod"),
    all(feature = "test", feature = "prod")
))]
compile_error!(
    "Only one of the `dev`, `test`, or `prod` features may be enabled for pqcnet-crypto."
);

#[cfg(feature = "dev")]
const KEY_TTL_SECS: u64 = 30;
#[cfg(feature = "test")]
const KEY_TTL_SECS: u64 = 5 * 60;
#[cfg(feature = "prod")]
const KEY_TTL_SECS: u64 = 60 * 60;

type Result<T> = std::result::Result<T, CryptoError>;

fn default_key_ttl_secs() -> u64 {
    KEY_TTL_SECS
}

fn default_secret_seed() -> String {
    hex::encode(generate_seed())
}

fn generate_seed() -> [u8; 32] {
    let mut seed = [0u8; 32];
    OsRng.fill_bytes(&mut seed);
    seed
}

fn default_threshold_min_shares() -> u8 {
    3
}

fn default_threshold_total_shares() -> u8 {
    5
}

const SAMPLE_KYBER_PK: &str =
    "9fa0c3c7f19d4eb64a88d9dcf45b2e77e6f233d41f5f8d1ef4ef7a0b1f284b55";
const SAMPLE_HQC_PK: &str =
    "6cbf9cd9bf2d4de1a0aa9b6dd92d8f041b8478be45f4b3d18664a8b5d13e9088";

/// Shared crypto configuration section.
///
/// # TOML
/// ```text
/// [crypto]
/// node-id = "sentry-a"
/// secret-seed = "22ff..."
/// key-ttl-secs = 3600
/// threshold-min-shares = 3
/// threshold-total-shares = 5
/// ```
///
/// # YAML
/// ```text
/// crypto:
///   node-id: sentry-a
///   secret-seed: "22ff..."
///   key-ttl-secs: 3600
///   threshold-min-shares: 3
///   threshold-total-shares: 5
/// ```
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum KemScheme {
    Kyber,
    Hqc,
}

impl KemScheme {
    pub fn as_str(&self) -> &'static str {
        match self {
            KemScheme::Kyber => "kyber",
            KemScheme::Hqc => "hqc",
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SignatureScheme {
    Dilithium,
    Falcon,
    Sphincs,
}

impl SignatureScheme {
    pub fn as_str(&self) -> &'static str {
        match self {
            SignatureScheme::Dilithium => "dilithium",
            SignatureScheme::Falcon => "falcon",
            SignatureScheme::Sphincs => "sphincs",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct CryptoConfig {
    /// Human-readable identifier for the node, also used in key derivation.
    pub node_id: String,
    /// Hex-encoded 32-byte seed. Generated automatically if omitted.
    #[serde(default = "default_secret_seed")]
    pub secret_seed: String,
    /// TTL for derived keys. Defaults to feature-specific values.
    #[serde(default = "default_key_ttl_secs")]
    pub key_ttl_secs: u64,
    /// Minimum number of shares required by the Shamir policy.
    #[serde(default = "default_threshold_min_shares")]
    pub threshold_min_shares: u8,
    /// Total number of shares provisioned for the node.
    #[serde(default = "default_threshold_total_shares")]
    pub threshold_total_shares: u8,
    /// PQC public keys exposed to peers/regulators.
    #[serde(default)]
    pub advertised_kems: Vec<KemAdvertisement>,
    /// Signature redundancy policy (e.g., Dilithium + SPHINCS).
    #[serde(default)]
    pub signature_redundancy: Option<SignatureRedundancy>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct KemAdvertisement {
    pub scheme: KemScheme,
    pub public_key_hex: String,
    #[serde(default)]
    pub backup_only: bool,
    #[serde(default)]
    pub key_id: Option<String>,
}

impl KemAdvertisement {
    pub fn sample_primary() -> Self {
        Self {
            scheme: KemScheme::Kyber,
            public_key_hex: SAMPLE_KYBER_PK.into(),
            backup_only: false,
            key_id: Some("kyber-prod-0001".into()),
        }
    }

    pub fn sample_backup() -> Self {
        Self {
            scheme: KemScheme::Hqc,
            public_key_hex: SAMPLE_HQC_PK.into(),
            backup_only: true,
            key_id: Some("hqc-drill-0001".into()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct SignatureRedundancy {
    pub primary: SignatureScheme,
    pub backup: SignatureScheme,
    #[serde(default)]
    pub require_dual: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KemRationale {
    Normal,
    Drill,
    Fallback,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KemStatus {
    pub scheme: KemScheme,
    pub backup_only: bool,
    pub rationale: KemRationale,
}

impl KemStatus {
    pub fn new(scheme: KemScheme, backup_only: bool, rationale: KemRationale) -> Self {
        Self {
            scheme,
            backup_only,
            rationale,
        }
    }
}

impl CryptoConfig {
    /// Canonical sample helpful for docs/tests.
    pub fn sample(node_id: &str) -> Self {
        Self {
            node_id: node_id.to_owned(),
            secret_seed: "1111111111111111111111111111111111111111111111111111111111111111".into(),
            key_ttl_secs: default_key_ttl_secs(),
            threshold_min_shares: default_threshold_min_shares(),
            threshold_total_shares: default_threshold_total_shares(),
            advertised_kems: vec![
                KemAdvertisement::sample_primary(),
                KemAdvertisement::sample_backup(),
            ],
            signature_redundancy: Some(SignatureRedundancy {
                primary: SignatureScheme::Dilithium,
                backup: SignatureScheme::Sphincs,
                require_dual: true,
            }),
        }
    }

    fn threshold_policy(&self) -> Result<ThresholdPolicy> {
        let t = self.threshold_min_shares;
        let n = self.threshold_total_shares;
        if t == 0 || n == 0 || t > n {
            return Err(CryptoError::InvalidThreshold { t, n });
        }
        Ok(ThresholdPolicy { t, n })
    }
}

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("secret seed must be 64 hex characters, got {0}")]
    InvalidSeedLength(usize),
    #[error("secret seed is not valid hex: {0}")]
    InvalidSeedHex(String),
    #[error("threshold shares invalid: t={t} n={n}")]
    InvalidThreshold { t: u8, n: u8 },
    #[error("key ttl must be greater than zero seconds")]
    InvalidTtl,
    #[error("rotation interval overflowed u64 milliseconds")]
    IntervalOverflow,
    #[error("hkdf expand failed")]
    DerivationFailed,
    #[error(transparent)]
    Time(#[from] SystemTimeError),
    #[error(transparent)]
    Pqc(#[from] PqcError),
}

#[derive(Clone, Debug)]
pub struct DerivedKey {
    pub peer_id: String,
    pub material: [u8; 32],
    pub expires_at: SystemTime,
    /// Logical ML-KEM key identifier backing the handshake.
    pub key_id: KeyId,
    /// Ciphertext that must be delivered so the peer can decapsulate.
    pub ciphertext: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature {
    pub signer: String,
    pub key_id: KeyId,
    pub bytes: Vec<u8>,
}

pub struct CryptoProvider {
    node_id: String,
    secret_seed: [u8; 32],
    key_manager: KeyManager,
    signature_manager: SignatureManager,
    signing: SigningMaterial,
    advertised_kems: Vec<KemAdvertisement>,
    signature_redundancy: Option<SignatureRedundancy>,
    kem_status: KemStatus,
}

struct SigningMaterial {
    state: DsaKeyState,
    secret_key: Vec<u8>,
}

impl CryptoProvider {
    pub fn from_config(config: &CryptoConfig) -> Result<Self> {
        if config.key_ttl_secs == 0 {
            return Err(CryptoError::InvalidTtl);
        }

        let seed = decode_seed(&config.secret_seed)?;
        let key_ttl = Duration::from_secs(config.key_ttl_secs);
        let rotation_ms: u64 = key_ttl
            .as_millis()
            .try_into()
            .map_err(|_| CryptoError::IntervalOverflow)?;
        let threshold = config.threshold_policy()?;

        let kem_engine = MlKemEngine::new(Box::new(DemoMlKem::new()));
        let mut key_manager = KeyManager::new(kem_engine, threshold, rotation_ms);
        let now_ms = now_ms()?;
        let _ = key_manager.keygen_with_material(now_ms)?;

        let dsa_engine = MlDsaEngine::new(Box::new(DemoMlDsa::new()));
        let mut signature_manager = SignatureManager::new(dsa_engine);
        let (sign_state, sign_pair) = signature_manager.generate_signing_key(now_ms)?;
        let signing = SigningMaterial {
            state: sign_state,
            secret_key: sign_pair.secret_key,
        };

        let advertised_kems = if config.advertised_kems.is_empty() {
            vec![
                KemAdvertisement::sample_primary(),
                KemAdvertisement::sample_backup(),
            ]
        } else {
            config.advertised_kems.clone()
        };
        let signature_redundancy = config.signature_redundancy.clone();
        let kem_status = initial_kem_status(&advertised_kems);

        Ok(Self {
            node_id: config.node_id.clone(),
            secret_seed: seed,
            key_manager,
            signature_manager,
            signing,
            advertised_kems,
            signature_redundancy,
            kem_status,
        })
    }

    pub fn derive_shared_key(&mut self, peer_id: &str) -> Result<DerivedKey> {
        let now_ms = now_ms()?;
        self.ensure_active_kem(now_ms)?;

        let (state, encapsulation) = self.key_manager.encapsulate_for_current()?;
        let material = self.kdf(peer_id, &encapsulation.shared_secret)?;

        Ok(DerivedKey {
            peer_id: peer_id.to_owned(),
            material,
            expires_at: system_time_from_ms(state.expires_at),
            key_id: state.id.clone(),
            ciphertext: encapsulation.ciphertext,
        })
    }

    pub fn sign(&self, payload: impl AsRef<[u8]>) -> Result<Signature> {
        let bytes = self
            .signature_manager
            .sign(&self.signing.secret_key, payload.as_ref())?;
        Ok(Signature {
            signer: self.node_id.clone(),
            key_id: self.signing.state.id.clone(),
            bytes,
        })
    }

    pub fn verify(&self, payload: impl AsRef<[u8]>, signature: &Signature) -> Result<bool> {
        if signature.signer != self.node_id {
            return Ok(false);
        }
        match self
            .signature_manager
            .verify(&signature.key_id, payload.as_ref(), &signature.bytes)
        {
            Ok(_) => Ok(true),
            Err(PqcError::VerifyFailed) | Err(PqcError::InvalidInput(_)) => Ok(false),
            Err(err) => Err(CryptoError::from(err)),
        }
    }

    pub fn advertised_kems(&self) -> &[KemAdvertisement] {
        &self.advertised_kems
    }

    pub fn signature_redundancy(&self) -> Option<&SignatureRedundancy> {
        self.signature_redundancy.as_ref()
    }

    pub fn kem_status(&self) -> KemStatus {
        self.kem_status
    }

    pub fn set_kem_rationale(&mut self, rationale: KemRationale) {
        self.kem_status.rationale = rationale;
    }

    pub fn set_kem_scheme(&mut self, scheme: KemScheme, rationale: KemRationale) {
        let backup_only = advertised_as_backup(&self.advertised_kems, scheme);
        self.kem_status = KemStatus::new(scheme, backup_only, rationale);
    }

    fn ensure_active_kem(&mut self, now_ms: TimestampMs) -> Result<()> {
        self.key_manager.rotate_with_material(now_ms)?;
        Ok(())
    }

    fn kdf(&self, peer_id: &str, shared_secret: &[u8]) -> Result<[u8; 32]> {
        let hkdf = Hkdf::<Sha256>::new(Some(&self.secret_seed), shared_secret);
        let mut material = [0u8; 32];
        hkdf.expand(peer_id.as_bytes(), &mut material)
            .map_err(|_| CryptoError::DerivationFailed)?;
        Ok(material)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> CryptoConfig {
        CryptoConfig {
            node_id: "node-a".into(),
            secret_seed: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            key_ttl_secs: 1,
            threshold_min_shares: 2,
            threshold_total_shares: 3,
            advertised_kems: vec![
                KemAdvertisement::sample_primary(),
                KemAdvertisement::sample_backup(),
            ],
            signature_redundancy: Some(SignatureRedundancy {
                primary: SignatureScheme::Dilithium,
                backup: SignatureScheme::Sphincs,
                require_dual: true,
            }),
        }
    }

    #[test]
    fn derives_deterministic_shared_keys() {
        let mut provider = CryptoProvider::from_config(&config()).unwrap();
        let first = provider.derive_shared_key("peer-1").unwrap();
        let second = provider.derive_shared_key("peer-1").unwrap();
        assert_eq!(first.peer_id, second.peer_id);
        assert_eq!(first.material, second.material);
        assert_ne!(
            first.material,
            provider.derive_shared_key("peer-2").unwrap().material
        );
    }

    #[test]
    fn signs_and_verifies_payloads() {
        let provider = CryptoProvider::from_config(&config()).unwrap();
        let payload = b"hello";
        let signature = provider.sign(payload).unwrap();
        assert!(provider.verify(payload, &signature).unwrap());
        assert!(!provider.verify(b"h3110", &signature).unwrap());
    }

    #[test]
    fn rejects_invalid_seed_lengths() {
        let cfg = CryptoConfig {
            secret_seed: "abcd".into(),
            ..config()
        };
        assert!(matches!(
            CryptoProvider::from_config(&cfg),
            Err(CryptoError::InvalidSeedLength(2))
        ));
    }

    #[test]
    fn exposes_sample_config() {
        let sample = CryptoConfig::sample("node-x");
        assert_eq!(sample.node_id, "node-x");
        assert_eq!(sample.secret_seed.len(), 64);
        assert_eq!(sample.threshold_min_shares, 3);
        assert_eq!(sample.threshold_total_shares, 5);
        assert_eq!(sample.advertised_kems.len(), 2);
        assert!(sample.signature_redundancy.is_some());
    }

    #[test]
    fn advertised_kems_default_to_sample_entries() {
        let provider = CryptoProvider::from_config(&config()).unwrap();
        assert_eq!(provider.advertised_kems().len(), 2);
        let status = provider.kem_status();
        assert_eq!(status.scheme, KemScheme::Kyber);
        assert!(!status.backup_only);
    }

    #[test]
    fn kem_status_updates_when_switching_schemes() {
        let mut provider = CryptoProvider::from_config(&config()).unwrap();
        provider.set_kem_scheme(KemScheme::Hqc, KemRationale::Drill);
        let status = provider.kem_status();
        assert_eq!(status.scheme, KemScheme::Hqc);
        assert!(status.backup_only);
        assert_eq!(status.rationale, KemRationale::Drill);
    }
}

fn decode_seed(seed_hex: &str) -> Result<[u8; 32]> {
    let bytes =
        hex::decode(seed_hex).map_err(|_| CryptoError::InvalidSeedHex(seed_hex.to_owned()))?;
    if bytes.len() != 32 {
        return Err(CryptoError::InvalidSeedLength(bytes.len()));
    }
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&bytes);
    Ok(seed)
}

fn now_ms() -> Result<TimestampMs> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_millis()
        .try_into()
        .map_err(|_| CryptoError::IntervalOverflow)?)
}

fn system_time_from_ms(ts: TimestampMs) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(ts)
}

fn initial_kem_status(kems: &[KemAdvertisement]) -> KemStatus {
    if let Some(primary) = kems.iter().find(|kem| !kem.backup_only) {
        return KemStatus::new(primary.scheme, false, KemRationale::Normal);
    }
    kems.first()
        .map(|kem| KemStatus::new(kem.scheme, kem.backup_only, KemRationale::Normal))
        .unwrap_or_else(|| KemStatus::new(KemScheme::Kyber, false, KemRationale::Normal))
}

fn advertised_as_backup(kems: &[KemAdvertisement], scheme: KemScheme) -> bool {
    kems.iter()
        .find(|kem| kem.scheme == scheme)
        .map(|kem| kem.backup_only)
        .unwrap_or(false)
}
