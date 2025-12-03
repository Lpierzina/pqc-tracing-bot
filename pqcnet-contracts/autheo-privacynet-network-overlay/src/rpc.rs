use autheo_pqcnet_5dqeh::{Icosuple, VertexId};
use autheo_privacynet::pipeline::PrivacyNetResponse;
use serde::{Deserialize, Serialize};

use crate::error::{OverlayError, OverlayResult};

#[derive(Clone, Debug)]
pub struct JsonRpcCall {
    pub id: u64,
    pub command: OverlayRpc,
}

#[derive(Clone, Debug)]
pub enum OverlayRpc {
    CreateVertex(CreateVertexParams),
    ProveAttribute(ProveAttributeParams),
    VerifyProof(VerifyProofParams),
    RevokeCredential(RevokeCredentialParams),
    QtaidTokenize(QtaidTokenizeParams),
}

pub fn decode_request(raw: &str) -> OverlayResult<JsonRpcCall> {
    let envelope: serde_json::Value = serde_json::from_str(raw)?;
    let id = envelope
        .get("id")
        .ok_or(OverlayError::MissingField("id"))?
        .as_u64()
        .ok_or(OverlayError::MissingField("id"))?;
    let method = envelope
        .get("method")
        .and_then(|m| m.as_str())
        .ok_or(OverlayError::MissingField("method"))?;
    let params = envelope
        .get("params")
        .cloned()
        .unwrap_or_else(|| serde_json::Value::Object(Default::default()));
    let command = match method {
        "privacynet_createVertex" => OverlayRpc::CreateVertex(serde_json::from_value(params)?),
        "privacynet_proveAttribute" => OverlayRpc::ProveAttribute(serde_json::from_value(params)?),
        "privacynet_verifyProof" => OverlayRpc::VerifyProof(serde_json::from_value(params)?),
        "privacynet_revokeCredential" => {
            OverlayRpc::RevokeCredential(serde_json::from_value(params)?)
        }
        "privacynet_qtaid_tokenize" => OverlayRpc::QtaidTokenize(serde_json::from_value(params)?),
        other => return Err(OverlayError::UnsupportedMethod(other.into())),
    };
    Ok(JsonRpcCall { id, command })
}

pub fn encode_success<T: Serialize>(id: u64, result: T) -> OverlayResult<String> {
    let response = JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result,
    };
    Ok(serde_json::to_string(&response)?)
}

pub fn encode_error(id: u64, code: i32, message: &str) -> OverlayResult<String> {
    let response = JsonRpcErrorResponse {
        jsonrpc: "2.0",
        id,
        error: JsonRpcError {
            code,
            message: message.into(),
        },
    };
    Ok(serde_json::to_string(&response)?)
}

#[derive(Clone, Debug, Deserialize)]
pub struct CreateVertexParams {
    pub icosuple_json: serde_json::Value,
    #[serde(default)]
    pub parent_ids: Vec<String>,
    #[serde(default)]
    pub label_override: Option<String>,
}

impl CreateVertexParams {
    pub fn materialize(&self) -> OverlayResult<Icosuple> {
        let mut icosuple: Icosuple = serde_json::from_value(self.icosuple_json.clone())?;
        if let Some(label) = &self.label_override {
            icosuple.label = label.clone();
        }
        Ok(icosuple)
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct ProveAttributeParams {
    pub did: String,
    pub attribute: String,
    pub witness: String,
    #[serde(default)]
    pub manifold: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct VerifyProofParams {
    #[serde(default)]
    pub proof_id: Option<String>,
    #[serde(default)]
    pub proof_object: Option<serde_json::Value>,
    #[serde(default)]
    pub vertex: Option<String>,
    #[serde(default)]
    pub include_telemetry: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RevokeCredentialParams {
    pub credential_id: String,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct QtaidTokenizeParams {
    pub genome_segment: String,
    pub owner_did: String,
    #[serde(default)]
    pub bits_per_snp: Option<u8>,
}

#[derive(Clone, Debug, Serialize)]
pub struct JsonRpcResponse<T> {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub result: T,
}

#[derive(Clone, Debug, Serialize)]
pub struct JsonRpcErrorResponse {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub error: JsonRpcError,
}

#[derive(Clone, Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct CreateVertexResult {
    pub vertex_id: String,
    pub parents: Vec<String>,
    pub payload_bytes: usize,
    pub entanglement: f32,
    pub ann_similarity: f32,
}

#[derive(Clone, Debug, Serialize)]
pub struct EntangledProofResult {
    pub proof_id: String,
    pub did: String,
    pub attribute: String,
    pub manifold: String,
    pub chsh_violation: f32,
    pub witness_commitment: String,
    pub response: PrivacyNetResponse,
}

#[derive(Clone, Debug, Serialize)]
pub struct VerifyProofResult {
    pub proof_id: String,
    pub valid: bool,
    pub vertex: Option<String>,
    pub telemetry: Option<ProofTelemetry>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProofTelemetry {
    pub verifier: String,
    pub verified_at_ms: u128,
    pub residual_budget: f64,
    pub global_latency_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct RevokeCredentialResult {
    pub credential_id: String,
    pub tx_hash: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct QtaidTokenizeResult {
    pub owner_did: String,
    pub allele_commitments: Vec<String>,
    pub bits_per_snp: u8,
}

pub fn vertex_to_hex(id: &VertexId) -> String {
    hex::encode(id.as_bytes())
}

pub fn vertex_from_hex(value: &str) -> OverlayResult<VertexId> {
    let bytes = hex::decode(value)?;
    if bytes.len() != 32 {
        return Err(OverlayError::state("vertex id must be 32 bytes"));
    }
    let mut data = [0u8; 32];
    data.copy_from_slice(&bytes);
    Ok(VertexId(data))
}
