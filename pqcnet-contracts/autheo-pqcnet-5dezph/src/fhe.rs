use blake3::Hasher;
use serde::{Deserialize, Serialize};
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
