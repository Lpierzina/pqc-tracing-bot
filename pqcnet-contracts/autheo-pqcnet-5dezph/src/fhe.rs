use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use blake3::Hasher;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use tfhe::shortint::{
    gen_keys, parameters::PARAM_MESSAGE_2_CARRY_2_KS_PBS, Ciphertext as ShortintCiphertext,
    ClientKey as ShortintClientKey, ServerKey as ShortintServerKey,
};
use thiserror::Error;

use crate::config::FheConfig;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FheCiphertext {
    pub digest: [u8; 32],
    pub slots: usize,
    pub scale: f64,
}

impl FheCiphertext {
    pub fn zero(scale: f64) -> Self {
        Self {
            digest: [0u8; 32],
            slots: 0,
            scale,
        }
    }
}

#[derive(Debug, Error, Clone)]
#[error("{0}")]
pub struct FheError(pub String);

impl FheError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }
}

pub trait FheEvaluator: Send + Sync {
    fn encrypt(&self, slots: &[f64]) -> Result<FheCiphertext, FheError>;
    fn aggregate(&self, ciphertexts: &[FheCiphertext]) -> Result<FheCiphertext, FheError>;
}

pub struct MockCkksEvaluator {
    config: FheConfig,
}

impl MockCkksEvaluator {
    pub fn new(config: FheConfig) -> Self {
        Self { config }
    }

    fn digest_from_slots(&self, slots: &[f64]) -> [u8; 32] {
        let mut hasher = Hasher::new();
        hasher.update(&self.config.polynomial_degree.to_le_bytes());
        for slot in slots {
            hasher.update(&slot.to_le_bytes());
        }
        let mut out = [0u8; 32];
        out.copy_from_slice(hasher.finalize().as_bytes());
        out
    }
}

impl FheEvaluator for MockCkksEvaluator {
    fn encrypt(&self, slots: &[f64]) -> Result<FheCiphertext, FheError> {
        if slots.is_empty() {
            return Err(FheError::new("no slots provided"));
        }
        Ok(FheCiphertext {
            digest: self.digest_from_slots(slots),
            slots: slots.len(),
            scale: self.config.ciphertext_scale,
        })
    }

    fn aggregate(&self, ciphertexts: &[FheCiphertext]) -> Result<FheCiphertext, FheError> {
        if ciphertexts.is_empty() {
            return Ok(FheCiphertext::zero(self.config.ciphertext_scale));
        }
        let mut hasher = Hasher::new();
        for ct in ciphertexts {
            if (ct.scale - self.config.ciphertext_scale).abs() > f64::EPSILON {
                return Err(FheError::new("ciphertext scale mismatch"));
            }
            hasher.update(&ct.digest);
            hasher.update(&ct.slots.to_le_bytes());
        }
        let mut digest = [0u8; 32];
        digest.copy_from_slice(hasher.finalize().as_bytes());
        Ok(FheCiphertext {
            digest,
            slots: ciphertexts.iter().map(|c| c.slots).sum(),
            scale: self.config.ciphertext_scale,
        })
    }
}

#[derive(Clone)]
struct PackedCiphertext {
    slots: Vec<ShortintCiphertext>,
    cleartext: Vec<u64>,
}

pub struct TfheCkksEvaluator {
    config: FheConfig,
    client_key: Arc<Mutex<ShortintClientKey>>,
    server_key: Arc<ShortintServerKey>,
    store: Arc<RwLock<HashMap<[u8; 32], PackedCiphertext>>>,
    nonce: Arc<AtomicU64>,
}

impl TfheCkksEvaluator {
    pub fn new(config: FheConfig) -> Self {
        let (client_key, server_key) = gen_keys(PARAM_MESSAGE_2_CARRY_2_KS_PBS);
        Self {
            config,
            client_key: Arc::new(Mutex::new(client_key)),
            server_key: Arc::new(server_key),
            store: Arc::new(RwLock::new(HashMap::new())),
            nonce: Arc::new(AtomicU64::new(1)),
        }
    }

    fn quantize(slot: f64) -> u64 {
        const MAX: f64 = 3.0;
        let clamped = slot.clamp(-1.0, 1.0);
        let normalized = (clamped + 1.0) / 2.0;
        (normalized * MAX).round().clamp(0.0, MAX) as u64
    }

    fn digest(&self, payload: &PackedCiphertext) -> [u8; 32] {
        let nonce = self.nonce.fetch_add(1, Ordering::Relaxed);
        let mut hasher = Hasher::new();
        hasher.update(&nonce.to_le_bytes());
        for value in &payload.cleartext {
            hasher.update(&value.to_le_bytes());
        }
        let mut digest = [0u8; 32];
        digest.copy_from_slice(hasher.finalize().as_bytes());
        digest
    }
}

impl FheEvaluator for TfheCkksEvaluator {
    fn encrypt(&self, slots: &[f64]) -> Result<FheCiphertext, FheError> {
        if slots.is_empty() {
            return Err(FheError::new("no slots provided"));
        }
        let mut payload = PackedCiphertext {
            slots: Vec::with_capacity(slots.len()),
            cleartext: Vec::with_capacity(slots.len()),
        };
        {
            let client = self.client_key.lock();
            for slot in slots {
                let value = Self::quantize(*slot);
                payload.slots.push(client.encrypt(value));
                payload.cleartext.push(value);
            }
        }
        let digest = self.digest(&payload);
        self.store.write().insert(digest, payload);
        Ok(FheCiphertext {
            digest,
            slots: slots.len(),
            scale: self.config.ciphertext_scale,
        })
    }

    fn aggregate(&self, ciphertexts: &[FheCiphertext]) -> Result<FheCiphertext, FheError> {
        if ciphertexts.is_empty() {
            return Ok(FheCiphertext::zero(self.config.ciphertext_scale));
        }
        let stored = {
            let guard = self.store.read();
            let mut payloads = Vec::with_capacity(ciphertexts.len());
            for ct in ciphertexts {
                if (ct.scale - self.config.ciphertext_scale).abs() > f64::EPSILON {
                    return Err(FheError::new("ciphertext scale mismatch"));
                }
                let packed = guard
                    .get(&ct.digest)
                    .cloned()
                    .ok_or_else(|| FheError::new("unknown ciphertext digest"))?;
                payloads.push(packed);
            }
            payloads
        };
        let mut accumulator = stored
            .first()
            .cloned()
            .ok_or_else(|| FheError::new("no ciphertexts to aggregate"))?;
        let server_key = &*self.server_key;
        for packed in stored.iter().skip(1) {
            if accumulator.slots.len() != packed.slots.len() {
                return Err(FheError::new("slot count mismatch during aggregation"));
            }
            for idx in 0..accumulator.slots.len() {
                let lhs = accumulator.slots[idx].clone();
                let rhs = &packed.slots[idx];
                accumulator.slots[idx] = server_key.unchecked_add(&lhs, rhs);
                accumulator.cleartext[idx] =
                    accumulator.cleartext[idx].saturating_add(packed.cleartext[idx]);
            }
        }
        let digest = self.digest(&accumulator);
        self.store.write().insert(digest, accumulator);
        Ok(FheCiphertext {
            digest,
            slots: ciphertexts[0].slots,
            scale: self.config.ciphertext_scale,
        })
    }
}
