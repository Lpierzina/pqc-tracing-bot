use autheo_dw3b_mesh::MeshError;
use autheo_pqc_core::error::PqcError;
use pqcnet_networking::NetworkingError;
use pqcnet_telemetry::TelemetryError;
use thiserror::Error;

pub type OverlayResult<T> = Result<T, OverlayError>;

#[derive(Debug, Error)]
pub enum OverlayError {
    #[error("json-rpc envelope missing field: {0}")]
    MissingField(&'static str),
    #[error("unsupported rpc method: {0}")]
    UnsupportedMethod(String),
    #[error("json serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("dw3b mesh error: {0}")]
    Mesh(#[from] MeshError),
    #[error("network client error: {0}")]
    Network(#[from] NetworkingError),
    #[error("telemetry error: {0}")]
    Telemetry(#[from] TelemetryError),
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
