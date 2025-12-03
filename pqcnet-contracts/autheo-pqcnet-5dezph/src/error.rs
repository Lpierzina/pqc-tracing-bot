use autheo_pqcnet_5dqeh::ModuleError;
use thiserror::Error;

use crate::{fhe::FheError, privacy::EzphPrivacyReport, zk::ZkError};

pub type EzphResult<T> = Result<T, EzphError>;

#[derive(Debug, Error)]
pub enum EzphError {
    #[error("privacy bounds violated")]
    PrivacyViolation { report: EzphPrivacyReport },
    #[error("projection rank must be >= 1, got {rank}")]
    InvalidProjectionRank { rank: usize },
    #[error("FHE evaluator error: {0}")]
    Fhe(#[from] FheError),
    #[error("ZK prover error: {0}")]
    Zk(#[from] ZkError),
    #[error(transparent)]
    Hypergraph(#[from] ModuleError),
}

impl From<EzphPrivacyReport> for EzphError {
    fn from(report: EzphPrivacyReport) -> Self {
        EzphError::PrivacyViolation { report }
    }
}
