use autheo_dw3b_mesh::{
    mesh::AnonymityProof, noise::NoiseSummary, MeshAnonymizeRequest, MeshAnonymizeResponse,
    MeshRoutePlan, QtaidProof, QtaidProveRequest,
};
use serde::{Deserialize, Serialize};

use crate::error::{OverlayError, OverlayResult};

#[derive(Clone, Debug)]
pub struct OverlayRpcEnvelope {
    pub id: u64,
    pub command: Dw3bOverlayRpc,
}

#[derive(Clone, Debug)]
pub enum Dw3bOverlayRpc {
    AnonymizeQuery(AnonymizeQueryParams),
    ObfuscateRoute(ObfuscateRouteParams),
    PolicyConfigure(PolicyConfigureParams),
    EntropyRequest(EntropyRequestParams),
    SyncState(SyncStateParams),
    QtaidProve(QtaidProveParams),
}

pub fn decode_request(raw: &str) -> OverlayResult<OverlayRpcEnvelope> {
    let envelope: serde_json::Value = serde_json::from_str(raw)?;
    let id = envelope
        .get("id")
        .and_then(|value| value.as_u64())
        .ok_or(OverlayError::MissingField("id"))?;
    let method = envelope
        .get("method")
        .and_then(|value| value.as_str())
        .ok_or(OverlayError::MissingField("method"))?;
    let params = envelope
        .get("params")
        .cloned()
        .unwrap_or_else(|| serde_json::Value::Object(Default::default()));
    let command = match method {
        "dw3b_anonymizeQuery" => Dw3bOverlayRpc::AnonymizeQuery(serde_json::from_value(params)?),
        "dw3b_obfuscateRoute" => Dw3bOverlayRpc::ObfuscateRoute(serde_json::from_value(params)?),
        "dw3b_policyConfigure" => Dw3bOverlayRpc::PolicyConfigure(serde_json::from_value(params)?),
        "dw3b_entropyRequest" => Dw3bOverlayRpc::EntropyRequest(serde_json::from_value(params)?),
        "dw3b_syncState" => Dw3bOverlayRpc::SyncState(serde_json::from_value(params)?),
        "dw3b_qtaidProve" => Dw3bOverlayRpc::QtaidProve(serde_json::from_value(params)?),
        other => return Err(OverlayError::UnsupportedMethod(other.into())),
    };
    Ok(OverlayRpcEnvelope { id, command })
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
pub struct AnonymizeQueryParams {
    pub did: String,
    pub attribute: String,
    pub payload: String,
    pub epsilon: f64,
    pub delta: f64,
    pub route_layers: u32,
    #[serde(default)]
    pub bloom_capacity: Option<u64>,
    #[serde(default)]
    pub bloom_fp_rate: Option<f64>,
    #[serde(default)]
    pub stake_threshold: Option<u64>,
    #[serde(default)]
    pub public_inputs: Vec<String>,
    #[serde(default)]
    pub lamport_hint: Option<u64>,
}

impl AnonymizeQueryParams {
    pub fn to_request(&self) -> MeshAnonymizeRequest {
        MeshAnonymizeRequest {
            did: self.did.clone(),
            attribute: self.attribute.clone(),
            payload: self.payload.clone(),
            epsilon: self.epsilon,
            delta: self.delta,
            route_layers: self.route_layers.max(1),
            bloom_capacity: self.bloom_capacity.unwrap_or(1 << 20),
            bloom_fp_rate: self.bloom_fp_rate.unwrap_or(0.005),
            stake_threshold: self.stake_threshold.unwrap_or(10_000),
            fhe_slots: vec![],
            public_inputs: self.public_inputs.clone(),
            lamport_hint: self.lamport_hint,
            poisson_lambda: 10.0,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct AnonymizeQueryResult {
    pub proof: AnonymityProof,
    pub route_plan: MeshRoutePlan,
    pub noise: NoiseSummary,
    pub compressed_payload_bytes: usize,
    pub chaos_lambda: f64,
}

impl From<MeshAnonymizeResponse> for AnonymizeQueryResult {
    fn from(response: MeshAnonymizeResponse) -> Self {
        Self {
            proof: response.proof,
            route_plan: response.route_plan,
            noise: response.noise,
            compressed_payload_bytes: response.compressed_payload.len(),
            chaos_lambda: response.chaos.lyapunov_exponent,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct ObfuscateRouteParams {
    pub data: String,
    pub layers: u32,
    pub k_anonymity: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ObfuscateRouteResult {
    pub routed: String,
    pub layers: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PolicyConfigureParams {
    pub policy_yaml: String,
    #[serde(default)]
    pub zkp_circuit: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PolicyConfigureResult {
    pub policy_hash: String,
    pub zkp_circuit: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct EntropyRequestParams {
    pub samples: u32,
    #[serde(default)]
    pub dimension5: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct EntropyRequestResult {
    pub vrbs: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SyncStateParams {
    #[serde(default)]
    pub causal_graph: Option<serde_json::Value>,
    #[serde(default)]
    pub force: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct SyncStateResult {
    pub lamport: u64,
    pub epoch: u64,
    pub session: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct QtaidProveParams {
    pub owner_did: String,
    pub trait_name: String,
    pub genome_segment: String,
    #[serde(default)]
    pub bits_per_snp: Option<u8>,
}

#[derive(Clone, Debug, Serialize)]
pub struct QtaidProveResult {
    pub tokens: Vec<String>,
    pub bits_per_snp: u8,
    pub proof_id: String,
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

pub fn encode_entropy(vrbs: Vec<[u8; 512]>) -> Vec<String> {
    vrbs.into_iter().map(|bytes| hex::encode(bytes)).collect()
}

pub fn to_qtaid_request(params: &QtaidProveParams) -> QtaidProveRequest {
    QtaidProveRequest {
        owner_did: params.owner_did.clone(),
        trait_name: params.trait_name.clone(),
        genome_segment: params.genome_segment.clone(),
        bits_per_snp: params.bits_per_snp.unwrap_or(4),
    }
}

pub fn qtaid_result(proof: QtaidProof) -> QtaidProveResult {
    QtaidProveResult {
        tokens: proof.tokens,
        bits_per_snp: proof.bits_per_snp,
        proof_id: proof.response.proof.proof_id,
    }
}
