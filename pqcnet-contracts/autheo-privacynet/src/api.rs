use serde::{Deserialize, Serialize};

use crate::{
    chaos::ChaosSample,
    dp::{Blake3Hash, DpMechanism, DpQuery},
    fhe::FheCircuitIntent,
};

/// Minimal JSON-RPC 2.0 envelope used by the Zer0veil shell extensions.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RpcEnvelope<T> {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub payload: T,
}

impl<T> RpcEnvelope<T> {
    pub const fn new(id: u64, payload: T) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            payload,
        }
    }
}

/// RPC surface documented in AUTHEO PRIMER Section 2.5.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum PrivacyNetRpc {
    PrivacynetDpQuery {
        query: DpQuery,
        budget: f64,
    },
    PrivacynetFheCompute {
        circuit: FheCircuitIntent,
        inputs: Vec<f64>,
    },
    PrivacynetChaosPerturb(ChaosPerturbRequest),
    PrivacynetQtaidProve {
        trait_name: String,
        genome: Blake3Hash,
        dp_params: DpQuery,
    },
    PrivacynetBudgetCompose {
        queries: Vec<Blake3Hash>,
        alpha: u8,
    },
}

/// Request/response helpers for the chaos perturbation endpoint.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChaosPerturbRequest {
    pub data: Vec<u8>,
    pub sensitivity: f64,
    pub mechanism: DpMechanism,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChaosPerturbResponse {
    pub perturbed: Vec<u8>,
    pub trajectory: ChaosSample,
}
