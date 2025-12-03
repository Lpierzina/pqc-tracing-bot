use autheo_pqc_core::error::PqcError;
use autheo_privacynet::errors::PrivacyNetError;
use pqcnet_networking::NetworkingError;
use thiserror::Error;

pub type OverlayResult<T> = Result<T, OverlayError>;

#[derive(Debug, Error)]
pub enum OverlayError {
    #[error("json-rpc envelope missing field: {0}")]
    MissingField(&'static str),
    #[error("json-rpc method unsupported: {0}")]
    UnsupportedMethod(String),
    #[error("json serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("privacy pipeline error: {0}")]
    Privacy(#[from] PrivacyNetError),
    #[error("network client error: {0}")]
    Network(#[from] NetworkingError),
    #[error("qstp tunnel error: {0}")]
    Qstp(#[from] PqcError),
    #[error("hex decoding error: {0}")]
    Hex(#[from] hex::FromHexError),
    #[error("grapplang parse error: {0}")]
    Grapplang(String),
    #[error("overlay state error: {0}")]
    State(String),
}

impl OverlayError {
    pub fn grapplang<S: Into<String>>(msg: S) -> Self {
        OverlayError::Grapplang(msg.into())
    }

    pub fn state<S: Into<String>>(msg: S) -> Self {
        OverlayError::State(msg.into())
    }
}
