use autheo_pqc_core::error::PqcError;
use autheo_pqc_core::handshake;
use autheo_pqc_core::runtime;
use autheo_pqc_core::types::TimestampMs;
use core::fmt;
use serde::{Deserialize, Serialize};

use crate::PqcBinding;

/// Signature bytes plus key metadata returned by the PQC runtime.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PqcSignature {
    pub key_id: String,
    pub bytes: Vec<u8>,
}

/// Request payload forwarded to the PQC handshake ABI.
#[derive(Clone, Debug)]
pub struct PqcHandshakeRequest {
    pub payload: Vec<u8>,
}

/// Binary envelope produced by `pqc_handshake`.
#[derive(Clone, Debug)]
pub struct PqcHandshakeReceipt {
    pub envelope: Vec<u8>,
}

/// Rotation metadata emitted after invoking `pqc_rotate`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PqcRotationOutcome {
    pub rotated: bool,
    pub old_key: Option<String>,
    pub new_key: Option<String>,
}

/// Errors surfaced while invoking PQC runtime ABIs.
#[derive(Debug, PartialEq, Eq)]
pub enum PqcRuntimeError {
    Disabled,
    Core(PqcError),
}

impl fmt::Display for PqcRuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PqcRuntimeError::Disabled => f.write_str("pqc runtime not configured"),
            PqcRuntimeError::Core(err) => write!(f, "pqc core error: {err}"),
        }
    }
}

impl std::error::Error for PqcRuntimeError {}

impl From<PqcError> for PqcRuntimeError {
    fn from(err: PqcError) -> Self {
        PqcRuntimeError::Core(err)
    }
}

/// Runtime contract used to call into `autheo-pqc-core` (native or wasm).
pub trait PqcRuntime: Send + Sync {
    fn pqc_handshake(
        &self,
        binding: &PqcBinding,
        request: &PqcHandshakeRequest,
    ) -> Result<PqcHandshakeReceipt, PqcRuntimeError>;

    fn pqc_sign(
        &self,
        binding: &PqcBinding,
        payload: &[u8],
    ) -> Result<PqcSignature, PqcRuntimeError>;

    fn pqc_rotate(
        &self,
        binding: &PqcBinding,
        now_ms: TimestampMs,
    ) -> Result<PqcRotationOutcome, PqcRuntimeError>;
}

/// Default runtime backed by `autheo-pqc-core`'s in-process contract state.
#[derive(Clone, Debug, Default)]
pub struct CorePqcRuntime;

impl CorePqcRuntime {
    pub fn new() -> Self {
        Self
    }
}

impl PqcRuntime for CorePqcRuntime {
    fn pqc_handshake(
        &self,
        _binding: &PqcBinding,
        request: &PqcHandshakeRequest,
    ) -> Result<PqcHandshakeReceipt, PqcRuntimeError> {
        if request.payload.is_empty() {
            return Err(PqcRuntimeError::Core(PqcError::InvalidInput(
                "handshake payload missing",
            )));
        }
        let mut buffer = vec![0u8; 8192];
        let written = handshake::execute_handshake(&request.payload, &mut buffer)
            .map_err(PqcRuntimeError::from)?;
        buffer.truncate(written);
        Ok(PqcHandshakeReceipt { envelope: buffer })
    }

    fn pqc_sign(
        &self,
        _binding: &PqcBinding,
        payload: &[u8],
    ) -> Result<PqcSignature, PqcRuntimeError> {
        let (signature, key_id) =
            runtime::with_contract_state(|state| -> Result<(Vec<u8>, String), PqcError> {
                let sig = state
                    .signature_manager
                    .sign(&state.signing_secret_key, payload)?;
                let key_hex = hex::encode(state.signing_key_state.id.0);
                Ok((sig, key_hex))
            })
            .map_err(PqcRuntimeError::from)?;
        Ok(PqcSignature {
            key_id,
            bytes: signature,
        })
    }

    fn pqc_rotate(
        &self,
        _binding: &PqcBinding,
        now_ms: TimestampMs,
    ) -> Result<PqcRotationOutcome, PqcRuntimeError> {
        runtime::with_contract_state(|state| -> Result<PqcRotationOutcome, PqcError> {
            let now = state.advance_time(Some(now_ms));
            let rotation = state.key_manager.rotate_if_needed(now)?;
            let (old_key, new_key, rotated) = match rotation {
                Some((old, new_state)) => (
                    Some(hex::encode(old.id.0)),
                    Some(hex::encode(new_state.id.0)),
                    true,
                ),
                None => (None, None, false),
            };
            Ok(PqcRotationOutcome {
                rotated,
                old_key,
                new_key,
            })
        })
        .map_err(PqcRuntimeError::from)
    }
}
