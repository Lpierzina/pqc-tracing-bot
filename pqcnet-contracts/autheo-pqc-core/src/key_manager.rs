//! ML-KEM key management (rotation + threshold policy metadata).

use crate::error::{PqcError, PqcResult};
use crate::kem::{MlKemEncapsulation, MlKemEngine, MlKemKeyPair};
use crate::types::{Bytes, KeyId, SecurityLevel, TimestampMs};

/// Threshold policy for Shamir-style sharing handled by the host.
#[derive(Clone, Copy, Debug)]
pub struct ThresholdPolicy {
    /// Minimum number of shares required to recover the secret.
    pub t: u8,
    /// Total number of provisioned shares.
    pub n: u8,
}

/// State for a single rotating ML-KEM key.
#[derive(Clone, Debug)]
pub struct KemKeyState {
    /// Logical identifier derived from the public key and creation time.
    pub id: KeyId,
    /// Serialized ML-KEM public key.
    pub public_key: Bytes,
    /// Security level for the key (e.g., ML-KEM-128).
    pub level: SecurityLevel,
    /// Timestamp when the key was created.
    pub created_at: TimestampMs,
    /// Timestamp when the key expires and must rotate.
    pub expires_at: TimestampMs,
}

/// Contract-level key manager (backed by storage in the host runtime).
///
/// # Example: key generation and rotation
///
/// ```ignore
/// # use autheo_pqc_core::key_manager::{KeyManager, ThresholdPolicy};
/// # use autheo_pqc_core::kem::{MlKem, MlKemEngine, MlKemEncapsulation, MlKemKeyPair};
/// # use autheo_pqc_core::types::{SecurityLevel, TimestampMs};
/// #
/// # struct DummyKem;
/// # impl MlKem for DummyKem {
/// #     fn level(&self) -> SecurityLevel { SecurityLevel::MlKem128 }
/// #     fn keygen(&self) -> autheo_pqc_core::error::PqcResult<MlKemKeyPair> {
/// #         Ok(MlKemKeyPair {
/// #             public_key: vec![0u8; 32],
/// #             secret_key: vec![1u8; 32],
/// #             level: SecurityLevel::MlKem128,
/// #         })
/// #     }
/// #     fn encapsulate(&self, _: &[u8]) -> autheo_pqc_core::error::PqcResult<MlKemEncapsulation> {
/// #         Ok(MlKemEncapsulation { ciphertext: vec![2u8; 32], shared_secret: vec![3u8; 32] })
/// #     }
/// #     fn decapsulate(&self, _: &[u8], _: &[u8]) -> autheo_pqc_core::error::PqcResult<Vec<u8>> {
/// #         Ok(vec![4u8; 32])
/// #     }
/// # }
/// #
/// let kem_impl = DummyKem;
/// let kem_engine = MlKemEngine::new(Box::new(kem_impl));
///
/// let mut km = KeyManager::new(
///     kem_engine,
///     ThresholdPolicy { t: 3, n: 5 },
///     300_000, // 300 seconds
/// );
///
/// let now: TimestampMs = 1_700_000_000_000;
/// let (state, _material) = km.keygen_with_material(now)?;
/// let rotation = km.rotate_if_needed(now + 301_000)?;
/// ```
#[derive(Clone, Debug)]
pub struct KemRotation {
    /// Expired state being replaced.
    pub old: KemKeyState,
    /// Freshly installed state.
    pub new: KemKeyState,
    /// Full ML-KEM key material corresponding to `new`.
    pub new_material: MlKemKeyPair,
}

pub struct KeyManager {
    kem: MlKemEngine,
    threshold: ThresholdPolicy,
    rotation_interval_ms: u64,
    current: Option<KemKeyState>,
}

impl KeyManager {
    /// Create a new ML-KEM key manager.
    pub fn new(kem: MlKemEngine, threshold: ThresholdPolicy, rotation_interval_ms: u64) -> Self {
        Self {
            kem,
            threshold,
            rotation_interval_ms,
            current: None,
        }
    }

    /// Return the threshold policy used for Shamir sharing off-chain.
    pub fn threshold_policy(&self) -> ThresholdPolicy {
        self.threshold
    }

    /// Return the configured rotation interval in milliseconds.
    pub fn rotation_interval_ms(&self) -> u64 {
        self.rotation_interval_ms
    }

    /// Generate new ML-KEM key pair and install as the current key.
    ///
    /// The host performs Shamir splitting + storage of the secret key shares.
    pub fn keygen_and_install(&mut self, now_ms: TimestampMs) -> PqcResult<KemKeyState> {
        let (state, _) = self.keygen_with_material(now_ms)?;
        Ok(state)
    }

    /// Generate a key pair, install it, and return the resulting material + metadata.
    pub fn keygen_with_material(
        &mut self,
        now_ms: TimestampMs,
    ) -> PqcResult<(KemKeyState, MlKemKeyPair)> {
        let pair: MlKemKeyPair = self.kem.keygen()?;
        let state = self.install_from_pair(&pair, now_ms);
        Ok((state, pair))
    }

    /// Rotate key if expired according to `rotation_interval_ms`.
    pub fn rotate_if_needed(
        &mut self,
        now_ms: TimestampMs,
    ) -> PqcResult<Option<(KemKeyState, KemKeyState)>> {
        Ok(self
            .rotate_with_material(now_ms)?
            .map(|rotation| (rotation.old, rotation.new)))
    }

    /// Rotate the key if needed and expose the new ML-KEM material.
    pub fn rotate_with_material(&mut self, now_ms: TimestampMs) -> PqcResult<Option<KemRotation>> {
        let current = match &self.current {
            Some(c) => c.clone(),
            None => {
                let (state, pair) = self.keygen_with_material(now_ms)?;
                return Ok(Some(KemRotation {
                    old: state.clone(),
                    new: state,
                    new_material: pair,
                }));
            }
        };

        if now_ms < current.expires_at {
            return Ok(None);
        }

        let old = current;
        let (new_state, new_pair) = self.keygen_with_material(now_ms)?;
        Ok(Some(KemRotation {
            old,
            new: new_state,
            new_material: new_pair,
        }))
    }

    /// Encapsulate a new session key to the current public key.
    pub fn encapsulate_for_current(&self) -> PqcResult<(KemKeyState, MlKemEncapsulation)> {
        let current = self
            .current
            .as_ref()
            .ok_or(PqcError::InternalError("no active KEM key"))?
            .clone();

        let enc = self.kem.encapsulate(&current.public_key)?;
        Ok((current, enc))
    }

    fn install_from_pair(&mut self, pair: &MlKemKeyPair, now_ms: TimestampMs) -> KemKeyState {
        let id = self.compute_key_id(&pair.public_key, now_ms);
        let state = KemKeyState {
            id,
            public_key: pair.public_key.clone(),
            level: pair.level,
            created_at: now_ms,
            expires_at: now_ms + self.rotation_interval_ms,
        };

        self.current = Some(state.clone());
        state
    }

    fn compute_key_id(&self, pk: &[u8], now_ms: TimestampMs) -> KeyId {
        use blake2::Blake2s256;
        use digest::{Digest, Output};

        let mut hasher = Blake2s256::new();
        hasher.update(pk);
        hasher.update(now_ms.to_le_bytes());
        let out: Output<Blake2s256> = hasher.finalize();

        let mut id = [0u8; 32];
        id.copy_from_slice(&out[..32]);
        KeyId(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::DemoMlKem;
    use crate::error::PqcError;
    use crate::kem::MlKemEngine;
    use alloc::boxed::Box;

    fn manager(rotation_interval_ms: u64) -> KeyManager {
        let engine = MlKemEngine::new(Box::new(DemoMlKem::new()));
        KeyManager::new(engine, ThresholdPolicy { t: 3, n: 5 }, rotation_interval_ms)
    }

    #[test]
    fn keygen_and_encapsulate_for_current_produces_shared_secret() {
        let mut km = manager(300);
        let now = 1_700_000_111_000;
        let current = km.keygen_and_install(now).expect("install");
        let (state, enc) = km.encapsulate_for_current().expect("encapsulate");
        assert_eq!(state.id, current.id);
        assert_eq!(enc.ciphertext.len(), 48);
        assert_eq!(enc.shared_secret.len(), 32);
    }

    #[test]
    fn encapsulate_without_key_returns_error() {
        let km = manager(100);
        let err = km.encapsulate_for_current().unwrap_err();
        assert_eq!(err, PqcError::InternalError("no active KEM key"));
    }

    #[test]
    fn rotate_if_needed_respects_interval() {
        let mut km = manager(100);
        let start = 1_700_000_000_000;
        let first = km.keygen_and_install(start).expect("install");
        assert!(km.rotate_if_needed(start + 50).unwrap().is_none());

        let rotation = km.rotate_if_needed(start + 101).unwrap();
        let (old, new) = rotation.expect("rotation result");
        assert_eq!(old.id, first.id);
        assert_ne!(new.id, first.id);
        assert!(new.created_at >= start + 101);
    }

    #[test]
    fn threshold_policy_and_interval_are_exposed() {
        let km = manager(250);
        let policy = km.threshold_policy();
        assert_eq!(policy.t, 3);
        assert_eq!(policy.n, 5);
        assert_eq!(km.rotation_interval_ms(), 250);
    }

    #[test]
    fn keygen_with_material_returns_secret_key() {
        let mut km = manager(400);
        let (state, pair) = km.keygen_with_material(1_800_000_000_000).expect("keygen");
        assert_eq!(state.public_key, pair.public_key);
        assert_eq!(state.level, pair.level);
        assert!(!pair.secret_key.is_empty());
    }
}
