use blake3::Hasher;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::config::ZkConfig;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ZkStatement {
    pub circuit_id: String,
    pub claim: String,
    pub public_inputs: Vec<String>,
}

impl ZkStatement {
    pub fn hash(&self) -> [u8; 32] {
        let mut hasher = Hasher::new();
        hasher.update(self.circuit_id.as_bytes());
        hasher.update(self.claim.as_bytes());
        for input in &self.public_inputs {
            hasher.update(input.as_bytes());
        }
        let mut out = [0u8; 32];
        out.copy_from_slice(hasher.finalize().as_bytes());
        out
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ZkProof {
    pub proof_system: String,
    pub curve: String,
    pub proof_bytes: Vec<u8>,
    pub statement_hash: [u8; 32],
    pub soundness_error: f64,
}

#[derive(Debug, Error, Clone)]
#[error("{0}")]
pub struct ZkError(pub String);

impl ZkError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }
}

pub trait ZkProver: Send + Sync {
    fn prove(&self, statement: &ZkStatement) -> Result<ZkProof, ZkError>;
}

pub struct MockCircomProver {
    config: ZkConfig,
}

impl MockCircomProver {
    pub fn new(config: ZkConfig) -> Self {
        Self { config }
    }
}

impl ZkProver for MockCircomProver {
    fn prove(&self, statement: &ZkStatement) -> Result<ZkProof, ZkError> {
        if statement.public_inputs.is_empty() {
            return Err(ZkError::new("public inputs missing"));
        }
        let mut hasher = Hasher::new();
        hasher.update(&statement.hash());
        hasher.update(self.config.proof_system.as_bytes());
        hasher.update(self.config.curve.as_bytes());
        let mut proof_bytes = vec![0u8; 96];
        proof_bytes.copy_from_slice(&hasher.finalize().as_bytes().repeat(3)[..96]);
        Ok(ZkProof {
            proof_system: self.config.proof_system.clone(),
            curve: self.config.curve.clone(),
            proof_bytes,
            statement_hash: statement.hash(),
            soundness_error: self.config.soundness,
        })
    }
}
