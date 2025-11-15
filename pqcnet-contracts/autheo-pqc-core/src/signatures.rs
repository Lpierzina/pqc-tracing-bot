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
/// let dsa_engine = MlDsaEngine::new(Box::new(DummyDsa));
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
pub struct SignatureManager {
    dsa: MlDsaEngine,
    /// Storage hook for public keys (simulated as in-memory).
    keys: Vec<DsaKeyState>,
}

impl SignatureManager {
    /// Create a new signature manager.
    pub fn new(dsa: MlDsaEngine) -> Self {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{DemoMlDsa, DemoMlKem};
    use crate::dsa::MlDsaEngine;
    use crate::error::PqcError;
    use crate::kem::MlKemEngine;
    use alloc::boxed::Box;

    fn manager() -> SignatureManager {
        let engine = MlDsaEngine::new(Box::new(DemoMlDsa::new()));
        SignatureManager::new(engine)
    }

    #[test]
    fn generate_sign_and_verify_flow_registers_key() {
        let mut mgr = manager();
        let now = 1_700_000_000_123;
        let (state, pair) = mgr.generate_signing_key(now).expect("keygen");
        let message = b"pqcnet message";
        let sig = mgr.sign(&pair.secret_key, message).expect("sign");
        mgr.verify(&state.id, message, &sig).expect("verify");
    }

    #[test]
    fn verify_with_unknown_key_id_fails() {
        let mgr = manager();
        let err = mgr
            .verify(&KeyId([0u8; 32]), b"msg", &[0u8; 32])
            .unwrap_err();
        assert_eq!(err, PqcError::InvalidInput("unknown key id"));
    }

    #[test]
    fn batch_verify_enforces_limits_and_succeeds() {
        let mut mgr = manager();
        let mut key_ids = Vec::new();
        let mut messages = Vec::new();
        let mut signatures = Vec::new();

        for i in 0..2 {
            let (state, pair) = mgr
                .generate_signing_key(1_700_000_000_000 + i as u64)
                .expect("keygen");
            let msg = vec![i as u8; 4];
            let sig = mgr.sign(&pair.secret_key, &msg).expect("sign");
            key_ids.push(state.id.clone());
            messages.push(msg);
            signatures.push(sig);
        }

        let err = mgr
            .batch_verify(1, &key_ids, &messages, &signatures)
            .unwrap_err();
        assert_eq!(err, PqcError::LimitExceeded("batch too large"));

        mgr.batch_verify(2, &key_ids, &messages, &signatures)
            .expect("batch verify succeeds");
    }

    #[test]
    fn sign_kem_transcript_matches_engine_transcript() {
        let mut mgr = manager();
        let (_, signing_pair) = mgr.generate_signing_key(1_700_555_000_000).expect("keygen");

        let kem_engine = MlKemEngine::new(Box::new(DemoMlKem::new()));
        let kem_pair = kem_engine.keygen().expect("kem keypair");
        let encapsulation = kem_engine.encapsulate(&kem_pair.public_key).expect("enc");
        let context = b"client=unit-test";

        let sig = mgr
            .sign_kem_transcript(&signing_pair.secret_key, &encapsulation, context)
            .expect("transcript signature");

        let mut transcript = Vec::new();
        transcript.extend_from_slice(&encapsulation.ciphertext);
        transcript.extend_from_slice(&encapsulation.shared_secret);
        transcript.extend_from_slice(context);

        let dsa = DemoMlDsa::new();
        let expected = dsa
            .sign(&signing_pair.secret_key, &transcript)
            .expect("direct sign");
        assert_eq!(sig, expected);
    }
}
