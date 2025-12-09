use std::collections::{HashMap, VecDeque};
use std::time::{Instant, SystemTime};

use autheo_dw3b_mesh::Dw3bMeshEngine;
use blake3::{hash, Hasher};
use pqcnet_networking::NetworkClient;
use pqcnet_qstp::MeshTransport;
use pqcnet_telemetry::{KemUsageReason, KemUsageRecord, TelemetryHandle};
use serde_json::Value;
use tracing::{info, warn};

use crate::{
    config::{Dw3bOverlayConfig, PqcOverlayConfig},
    error::OverlayResult,
    rpc::{
        decode_request, encode_error, encode_success, qtaid_result, to_qtaid_request,
        AnonymizeQueryParams, AnonymizeQueryResult, Dw3bOverlayRpc, EntropyRequestParams,
        EntropyRequestResult, ObfuscateRouteParams, ObfuscateRouteResult, PolicyConfigureParams,
        PolicyConfigureResult, QtaidProveParams, SyncStateParams, SyncStateResult,
    },
    transport::{Dw3bGateway, OverlayFrame},
};

pub struct Dw3bOverlayNode<T: MeshTransport> {
    config: Dw3bOverlayConfig,
    engine: Dw3bMeshEngine,
    network: NetworkClient,
    telemetry: TelemetryHandle,
    gateway: Dw3bGateway<T>,
    state: OverlayState,
}

impl<T: MeshTransport> Dw3bOverlayNode<T> {
    pub fn new(config: Dw3bOverlayConfig, gateway: Dw3bGateway<T>) -> Self {
        let network = NetworkClient::from_config(&config.node_id, config.networking.clone());
        let telemetry = TelemetryHandle::from_config(config.telemetry.clone());
        let engine = Dw3bMeshEngine::new(config.mesh.clone());
        record_pqc_inventory(&telemetry, &config.pqc);
        Self {
            config,
            engine,
            network,
            telemetry,
            gateway,
            state: OverlayState::default(),
        }
    }

    pub fn handle_jsonrpc(&mut self, raw: &str) -> OverlayResult<String> {
        let call = decode_request(raw)?;
        match self.dispatch(call.command) {
            Ok(result) => encode_success(call.id, result),
            Err(err) => {
                warn!("dw3b overlay error err={err}");
                encode_error(call.id, -32000, &err.to_string())
            }
        }
    }

    pub fn try_handle_qstp(&mut self) -> OverlayResult<Option<Value>> {
        if let Some(request) = self.gateway.try_recv_json()? {
            let response_raw = self.handle_jsonrpc(&request.to_string())?;
            let response_json: Value = serde_json::from_str(&response_raw)?;
            self.gateway.seal_json(&response_json)?;
            Ok(Some(response_json))
        } else {
            Ok(None)
        }
    }

    fn dispatch(&mut self, command: Dw3bOverlayRpc) -> OverlayResult<Value> {
        match command {
            Dw3bOverlayRpc::AnonymizeQuery(params) => self.handle_anonymize(params),
            Dw3bOverlayRpc::ObfuscateRoute(params) => self.handle_obfuscate(params),
            Dw3bOverlayRpc::PolicyConfigure(params) => self.handle_policy(params),
            Dw3bOverlayRpc::EntropyRequest(params) => self.handle_entropy(params),
            Dw3bOverlayRpc::SyncState(params) => self.handle_sync(params),
            Dw3bOverlayRpc::QtaidProve(params) => self.handle_qtaid(params),
        }
    }

    fn handle_anonymize(&mut self, params: AnonymizeQueryParams) -> OverlayResult<Value> {
        let mut request = params.to_request();
        let epsilon_cap = self
            .config
            .mesh
            .privacy
            .budget
            .session_epsilon
            .min(self.config.rpc.epsilon_cap);
        request.epsilon = request.epsilon.min(epsilon_cap);
        let delta_cap = self.config.mesh.privacy.budget.session_delta;
        request.delta = request.delta.min(delta_cap);
        request.bloom_fp_rate = request.bloom_fp_rate.min(self.config.rpc.bloom_fp_max);
        request.stake_threshold = request.stake_threshold.max(self.config.rpc.stake_threshold);
        let did = params.did.clone();
        let attribute = params.attribute.clone();
        let started = Instant::now();
        let response = self.engine.anonymize_query(request)?;
        let elapsed = started.elapsed().as_millis() as u64;
        let _ = self.telemetry.record_latency_ms("dw3b.anonymize", elapsed);
        let proof_id = response.proof.proof_id.clone();
        self.state.record_proof(proof_id.clone());
        self.broadcast_event(&OverlayFrame::ProofGenerated {
            did,
            attribute,
            proof_id,
        })?;
        Ok(serde_json::to_value(AnonymizeQueryResult::from(response))?)
    }

    fn handle_obfuscate(&mut self, params: ObfuscateRouteParams) -> OverlayResult<Value> {
        let routed = self.engine.obfuscate_route(
            params.data.as_bytes(),
            params.layers,
            params.k_anonymity,
        )?;
        let fingerprint = hex::encode(hash(&routed).as_bytes());
        self.broadcast_event(&OverlayFrame::RouteObfuscated {
            layers: params.layers,
            fingerprint: fingerprint.clone(),
        })?;
        let result = ObfuscateRouteResult {
            routed: hex::encode(routed),
            layers: params.layers,
        };
        Ok(serde_json::to_value(result)?)
    }

    fn handle_policy(&mut self, params: PolicyConfigureParams) -> OverlayResult<Value> {
        let mut hasher = Hasher::new();
        hasher.update(params.policy_yaml.as_bytes());
        if let Some(circuit) = &params.zkp_circuit {
            hasher.update(circuit.as_bytes());
        }
        let policy_hash = hex::encode(hasher.finalize().as_bytes());
        self.state.record_policy(policy_hash.clone());
        self.broadcast_event(&OverlayFrame::PolicyConfigured {
            policy_hash: policy_hash.clone(),
        })?;
        Ok(serde_json::to_value(PolicyConfigureResult {
            policy_hash,
            zkp_circuit: params.zkp_circuit,
        })?)
    }

    fn handle_entropy(&mut self, params: EntropyRequestParams) -> OverlayResult<Value> {
        let samples = self
            .engine
            .entropy_beacon(params.samples, params.dimension5);
        self.broadcast_event(&OverlayFrame::EntropyBeacon {
            samples: samples.len(),
        })?;
        Ok(serde_json::to_value(EntropyRequestResult {
            vrbs: crate::rpc::encode_entropy(samples),
        })?)
    }

    fn handle_sync(&mut self, _params: SyncStateParams) -> OverlayResult<Value> {
        Ok(serde_json::to_value(SyncStateResult {
            lamport: self.state.lamport,
            epoch: self.state.epoch,
            session: self.state.session,
        })?)
    }

    fn handle_qtaid(&mut self, params: QtaidProveParams) -> OverlayResult<Value> {
        let request = to_qtaid_request(&params);
        let proof = self.engine.qtaid_prove(request)?;
        self.broadcast_event(&OverlayFrame::QtaidProof {
            owner_did: params.owner_did.clone(),
            token_count: proof.tokens.len(),
        })?;
        Ok(serde_json::to_value(qtaid_result(proof))?)
    }

    fn broadcast_event(&self, frame: &OverlayFrame) -> OverlayResult<()> {
        let payload = serde_json::to_vec(frame)?;
        info!("dw3b overlay event={:?}", frame);
        let receipts = self.network.broadcast(&payload)?;
        let _ = self
            .telemetry
            .record_counter("dw3b.overlay.broadcast_bytes", payload.len() as u64);
        let _ = self
            .telemetry
            .record_counter("dw3b.overlay.broadcast_peers", receipts.len() as u64);
        Ok(())
    }
}

fn record_pqc_inventory(handle: &TelemetryHandle, pqc: &PqcOverlayConfig) {
    for kem in &pqc.advertised_kems {
        let reason = if kem.backup_only {
            KemUsageReason::Drill
        } else {
            KemUsageReason::Normal
        };
        handle.record_kem_event(KemUsageRecord {
            label: format!("dw3b-overlay::{}", kem.scheme.as_str()),
            scheme: kem.scheme.as_str().into(),
            reason,
            backup_only: kem.backup_only,
        });
    }
    if let Some(plan) = &pqc.signature_redundancy {
        let label = format!(
            "dw3b-overlay::sig::{}-{}",
            plan.primary.as_str(),
            plan.backup.as_str()
        );
        handle.record_kem_event(KemUsageRecord {
            label,
            scheme: "signature-stack".into(),
            reason: KemUsageReason::Normal,
            backup_only: plan.require_dual,
        });
    }
}

#[derive(Default)]
struct OverlayState {
    lamport: u64,
    epoch: u64,
    session: u64,
    policies: VecDeque<String>,
    proofs: HashMap<String, SystemTime>,
}

impl OverlayState {
    fn record_proof(&mut self, proof: String) {
        self.session = self.session.wrapping_add(1).max(1);
        self.lamport = self.lamport.wrapping_add(1).max(1);
        self.epoch = self.epoch.wrapping_add(1).max(1);
        self.proofs.insert(proof, SystemTime::now());
        while self.proofs.len() > 128 {
            if let Some(key) = self.proofs.keys().next().cloned() {
                self.proofs.remove(&key);
            } else {
                break;
            }
        }
    }

    fn record_policy(&mut self, hash: String) {
        if self.policies.len() >= 64 {
            self.policies.pop_back();
        }
        self.policies.push_front(hash);
    }
}
