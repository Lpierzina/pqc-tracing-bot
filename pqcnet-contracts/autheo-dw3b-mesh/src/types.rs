use std::io;

use autheo_privacynet::errors::PrivacyNetError;
use thiserror::Error;

pub type MeshResult<T> = Result<T, MeshError>;

/// Error surface for the DW3B mesh engine.
#[derive(Debug, Error)]
pub enum MeshError {
    #[error("privacy net error: {0}")]
    PrivacyNet(#[from] PrivacyNetError),
    #[error("compression error: {0}")]
    Compression(#[from] io::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("invalid parameter: {0}")]
    InvalidParameter(String),
    #[error("entropy pool exhausted")]
    EntropyDepleted,
}
