//! High-level crypto primitives shared by pqcnet binaries.
//! Provides deterministic key derivation, signing, and verification built
//! on top of SHA-256 so integration tests can exercise higher-level logic
//! without depending on external HSMs.
//!
//! # Quickstart
//! ```
//! use pqcnet_crypto::{CryptoConfig, CryptoProvider};
//!
//! let provider =
//!     CryptoProvider::from_config(&CryptoConfig::sample("demo-sentry")).unwrap();
//! let payload = b"doc-test";
//! let signature = provider.sign(payload);
//! assert!(provider.verify(payload, &signature));
//! ```

use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{Duration, SystemTime};
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

/// Shared crypto configuration section.
///
/// # TOML
/// ```text
/// [crypto]
/// node-id = "sentry-a"
/// secret-seed = "22ff..."
/// key-ttl-secs = 3600
/// ```
///
/// # YAML
/// ```text
/// crypto:
///   node-id: sentry-a
///   secret-seed: "22ff..."
///   key-ttl-secs: 3600
/// ```
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
}

impl CryptoConfig {
    /// Canonical sample helpful for docs/tests.
    pub fn sample(node_id: &str) -> Self {
        Self {
            node_id: node_id.to_owned(),
            secret_seed: "1111111111111111111111111111111111111111111111111111111111111111".into(),
            key_ttl_secs: default_key_ttl_secs(),
        }
    }
}

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("secret seed must be 64 hex characters, got {0}")]
    InvalidSeedLength(usize),
    #[error("secret seed is not valid hex: {0}")]
    InvalidSeedHex(String),
}

#[derive(Clone, Debug)]
pub struct KeyMaterial {
    pub node_id: String,
    pub secret_seed: [u8; 32],
}

impl KeyMaterial {
    pub fn from_config(config: &CryptoConfig) -> Result<Self, CryptoError> {
        let bytes = hex::decode(&config.secret_seed)
            .map_err(|_| CryptoError::InvalidSeedHex(config.secret_seed.clone()))?;
        if bytes.len() != 32 {
            return Err(CryptoError::InvalidSeedLength(bytes.len()));
        }
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&bytes);
        Ok(Self {
            node_id: config.node_id.clone(),
            secret_seed: seed,
        })
    }
}

#[derive(Clone, Debug)]
pub struct DerivedKey {
    pub peer_id: String,
    pub material: [u8; 32],
    pub expires_at: SystemTime,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature {
    pub signer: String,
    pub digest: [u8; 32],
}

pub struct CryptoProvider {
    material: KeyMaterial,
    key_ttl: Duration,
}

impl CryptoProvider {
    pub fn from_config(config: &CryptoConfig) -> Result<Self, CryptoError> {
        Ok(Self {
            material: KeyMaterial::from_config(config)?,
            key_ttl: Duration::from_secs(config.key_ttl_secs),
        })
    }

    pub fn derive_shared_key(&self, peer_id: &str) -> DerivedKey {
        let mut hasher = Sha256::new();
        hasher.update(&self.material.secret_seed);
        hasher.update(self.material.node_id.as_bytes());
        hasher.update(peer_id.as_bytes());
        let digest = hasher.finalize();
        let mut material = [0u8; 32];
        material.copy_from_slice(&digest);
        DerivedKey {
            peer_id: peer_id.to_owned(),
            material,
            expires_at: SystemTime::now() + self.key_ttl,
        }
    }

    pub fn sign(&self, payload: impl AsRef<[u8]>) -> Signature {
        let mut hasher = Sha256::new();
        hasher.update(&self.material.secret_seed);
        hasher.update(payload.as_ref());
        let digest = hasher.finalize();
        let mut digest_bytes = [0u8; 32];
        digest_bytes.copy_from_slice(&digest);
        Signature {
            signer: self.material.node_id.clone(),
            digest: digest_bytes,
        }
    }

    pub fn verify(&self, payload: impl AsRef<[u8]>, signature: &Signature) -> bool {
        signature.signer == self.material.node_id && self.sign(payload).digest == signature.digest
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
        }
    }

    #[test]
    fn derives_deterministic_shared_keys() {
        let provider = CryptoProvider::from_config(&config()).unwrap();
        let first = provider.derive_shared_key("peer-1");
        let second = provider.derive_shared_key("peer-1");
        assert_eq!(first.peer_id, second.peer_id);
        assert_eq!(first.material, second.material);
        assert_ne!(
            first.material,
            provider.derive_shared_key("peer-2").material
        );
    }

    #[test]
    fn signs_and_verifies_payloads() {
        let provider = CryptoProvider::from_config(&config()).unwrap();
        let payload = b"hello";
        let signature = provider.sign(payload);
        assert!(provider.verify(payload, &signature));
        assert!(!provider.verify(b"h3110", &signature));
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
    }
}
