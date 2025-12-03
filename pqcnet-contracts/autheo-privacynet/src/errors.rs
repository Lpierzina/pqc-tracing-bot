use autheo_pqcnet_5dezph::{error::EzphError, fhe::FheError};
use autheo_pqcnet_5dqeh::ModuleError;
use thiserror::Error;

use crate::{budget::BudgetError, dp::DpError};

pub type PrivacyNetResult<T> = Result<T, PrivacyNetError>;

#[derive(Debug, Error)]
pub enum PrivacyNetError {
    #[error(transparent)]
    Ezph(#[from] EzphError),
    #[error(transparent)]
    Module(#[from] ModuleError),
    #[error(transparent)]
    Fhe(#[from] FheError),
    #[error(transparent)]
    Budget(#[from] BudgetError),
    #[error(transparent)]
    DifferentialPrivacy(#[from] DpError),
    #[error("invalid rpc payload: {0}")]
    InvalidRpc(&'static str),
    #[error("request payload exceeded limit: {bytes} bytes > {limit}")]
    PayloadTooLarge { bytes: usize, limit: usize },
}
