//! ML-DSA signature management for PQCNet validators and actors.

use crate::dsa::{MlDsaEngine, MlDsaKeyPair};
use crate::error::{PqcError, PqcResult};
use crate::kem::MlKemEncapsulation;
use crate::types::{Bytes, KeyId, SecurityLevel, TimestampMs};
use alloc::vec::Vec;

/// Logical state for a signing key (e.g., for a validator).
#[derive(Clone)]
pub struct DsaKeyState {
    /// Logical identifier derived from the public key and timestamp.
    pub id: KeyId,
    /// Serialized ML-DSA public key.
    pub public_key: Bytes,
    /// Security level (e.g., ML-DSA-128).
    pub level: SecurityLevel,
    /// Creation timestamp.
    pub created_at: TimestampMs,
}

/// Signature contract façade.
///
/// # Example: ML-DSA sign / verify / batch
///
/// ```ignore
/// # use autheo_pqc_core::signatures::SignatureManager;
/// # use autheo_pqc_core::dsa::{MlDsa, MlDsaEngine, MlDsaKeyPair};
/// # use autheo_pqc_core::types::{SecurityLevel, TimestampMs, Bytes, KeyId};
/// # use autheo_pqc_core::error::PqcResult;
/// #
/// # struct DummyDsa;
/// # impl MlDsa for DummyDsa {
/// #     fn level(&self) -> SecurityLevel { SecurityLevel::MlDsa128 }
/// #     fn keygen(&self) -> PqcResult<MlDsaKeyPair> {
/// #         Ok(MlDsaKeyPair {
/// #             public_key: vec![0u8; 32],
/// #             secret_key: vec![1u8; 32],
/// #             level: SecurityLevel::MlDsa128,
/// #         })
/// #     }
/// #     fn sign(&self, _: &[u8], message: &[u8]) -> PqcResult<Bytes> {
/// #         Ok(message.to_vec())
/// #     }
/// #     fn verify(&self, _: &[u8], message: &[u8], signature: &[u8]) -> PqcResult<()> {
/// #         if message == signature { Ok(()) } else { Err(autheo_pqc_core::error::PqcError::VerifyFailed) }
/// #     }
/// # }
/// #
/// let dsa_engine = MlDsaEngine::new(&DummyDsa);
/// let mut sig_mgr = SignatureManager::new(dsa_engine);
///
/// let now: TimestampMs = 1_700_000_000_000;
/// let (key_state, keypair) = sig_mgr.generate_signing_key(now)?;
///
/// let msg = b\"hello, quantum world\".to_vec();
/// let sig = sig_mgr.sign(&keypair.secret_key, &msg)?;
///
/// sig_mgr.verify(&key_state.id, &msg, &sig)?;
/// ```
pub struct SignatureManager<'a> {
    dsa: MlDsaEngine<'a>,
    /// Storage hook for public keys (simulated as in-memory).
    keys: Vec<DsaKeyState>,
}

impl<'a> SignatureManager<'a> {
    /// Create a new signature manager.
    pub fn new(dsa: MlDsaEngine<'a>) -> Self {
        Self {
            dsa,
            keys: Vec::new(),
        }
    }

    /// Generate a signing key and return both the state and full key pair.
    pub fn generate_signing_key(
        &mut self,
        now_ms: TimestampMs,
    ) -> PqcResult<(DsaKeyState, MlDsaKeyPair)> {
        let pair = self.dsa.keygen()?;

        let id = self.compute_key_id(&pair.public_key, now_ms);

        let state = DsaKeyState {
            id,
            public_key: pair.public_key.clone(),
            level: pair.level,
            created_at: now_ms,
        };

        self.keys.push(state.clone());
        Ok((state, pair))
    }

    /// Sign arbitrary data with the provided secret key.
    pub fn sign(&self, sk: &[u8], msg: &[u8]) -> PqcResult<Bytes> {
        self.dsa.sign(sk, msg)
    }

    /// Verify using a registered public key.
    pub fn verify(&self, key_id: &KeyId, msg: &[u8], sig: &[u8]) -> PqcResult<()> {
        let state = self
            .keys
            .iter()
            .find(|k| &k.id == key_id)
            .ok_or(PqcError::InvalidInput("unknown key id"))?;
        self.dsa.verify(&state.public_key, msg, sig)
    }

    /// Batch verification across multiple keys/payloads.
    pub fn batch_verify(
        &self,
        max_batch: usize,
        key_ids: &[KeyId],
        messages: &[Bytes],
        signatures: &[Bytes],
    ) -> PqcResult<()> {
        let n = key_ids.len();
        if n == 0 || n != messages.len() || n != signatures.len() {
            return Err(PqcError::InvalidInput("batch dimension mismatch"));
        }

        let mut pks = Vec::with_capacity(n);
        for id in key_ids {
            let state = self
                .keys
                .iter()
                .find(|k| &k.id == id)
                .ok_or(PqcError::InvalidInput("unknown key id in batch"))?;
            pks.push(state.public_key.clone());
        }

        self.dsa.batch_verify(max_batch, &pks, messages, signatures)
    }

    /// Combined flow: sign a KEM encapsulation transcript atomically.
    ///
    /// This signs `ciphertext || shared_secret || context` as one operation.
    pub fn sign_kem_transcript(
        &self,
        sk: &[u8],
        kem: &MlKemEncapsulation,
        context: &[u8],
    ) -> PqcResult<Bytes> {
        let mut transcript =
            Vec::with_capacity(kem.ciphertext.len() + kem.shared_secret.len() + context.len());
        transcript.extend_from_slice(&kem.ciphertext);
        transcript.extend_from_slice(&kem.shared_secret);
        transcript.extend_from_slice(context);

        self.dsa.sign(sk, &transcript)
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
