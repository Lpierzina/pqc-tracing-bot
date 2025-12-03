use std::collections::{HashMap, VecDeque};
use std::convert::TryInto;
use std::time::{Instant, SystemTime};

use autheo_pqcnet_5dqeh::{Icosuple, VertexId};
use autheo_privacynet::{
    dp::{Blake3Hash, DpQuery},
    pipeline::{PrivacyNetEngine, PrivacyNetRequest, PrivacyNetResponse},
};
use blake3::Hasher;
use pqcnet_networking::NetworkClient;
use pqcnet_qstp::MeshTransport;
use pqcnet_telemetry::TelemetryHandle;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use serde_json::Value;
use tracing::{info, warn};

use crate::{
    config::OverlayNodeConfig,
    error::{OverlayError, OverlayResult},
    rpc::{
        decode_request, encode_error, encode_success, vertex_from_hex, vertex_to_hex,
        CreateVertexParams, CreateVertexResult, EntangledProofResult, OverlayRpc, ProofTelemetry,
        ProveAttributeParams, QtaidTokenizeParams, QtaidTokenizeResult, RevokeCredentialParams,
        RevokeCredentialResult, VerifyProofParams, VerifyProofResult,
    },
    transport::{OverlayFrame, QstpGateway},
};

const RECENT_VERTEX_CAP: usize = 32;
const MAX_PROOFS: usize = 1024;
const DEFAULT_DELTA: f64 = 1e-9;

pub struct OverlayNode<T: MeshTransport> {
    config: OverlayNodeConfig,
    engine: PrivacyNetEngine,
    network: NetworkClient,
    telemetry: TelemetryHandle,
    gateway: QstpGateway<T>,
    state: OverlayState,
}

impl<T: MeshTransport> OverlayNode<T> {
    pub fn new(config: OverlayNodeConfig, gateway: QstpGateway<T>) -> Self {
        let network = NetworkClient::from_config(&config.node_id, config.networking.clone());
        let telemetry = TelemetryHandle::from_config(config.telemetry.clone());
        let state = OverlayState::new(&config.node_id);
        Self {
            engine: PrivacyNetEngine::new(config.privacynet.clone()),
            config,
            network,
            telemetry,
            gateway,
            state,
        }
    }

    pub fn handle_jsonrpc(&mut self, raw: &str) -> OverlayResult<String> {
        let call = decode_request(raw)?;
        match self.dispatch(call.command) {
            Ok(result) => encode_success(call.id, result),
            Err(err) => {
                warn!("overlay rpc error err={err}");
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

    fn dispatch(&mut self, command: OverlayRpc) -> OverlayResult<Value> {
        match command {
            OverlayRpc::CreateVertex(params) => {
                let result = self.create_vertex(params)?;
                Ok(serde_json::to_value(result)?)
            }
            OverlayRpc::ProveAttribute(params) => {
                let result = self.prove_attribute(params)?;
                Ok(serde_json::to_value(result)?)
            }
            OverlayRpc::VerifyProof(params) => {
                let result = self.verify_proof(params)?;
                Ok(serde_json::to_value(result)?)
            }
            OverlayRpc::RevokeCredential(params) => {
                let result = self.revoke_credential(params)?;
                Ok(serde_json::to_value(result)?)
            }
            OverlayRpc::QtaidTokenize(params) => {
                let result = self.qtaid_tokenize(params)?;
                Ok(serde_json::to_value(result)?)
            }
        }
    }

    fn create_vertex(&mut self, params: CreateVertexParams) -> OverlayResult<CreateVertexResult> {
        let icosuple: Icosuple = params.materialize()?;
        let parents = if params.parent_ids.is_empty() {
            self.state.recent_parents(4)
        } else {
            params
                .parent_ids
                .iter()
                .map(|hex| vertex_from_hex(hex))
                .collect::<Result<Vec<_>, _>>()?
        };
        let vertex_id = icosuple.vertex_id(&parents);
        self.state.record_vertex(vertex_id);
        self.broadcast_event(&OverlayFrame::VertexCreated {
            vertex_id: vertex_to_hex(&vertex_id),
            label: icosuple.label.clone(),
            payload_bytes: icosuple.payload_bytes,
        })?;
        let avg_similarity = if icosuple.vector_signature.is_empty() {
            0.0
        } else {
            icosuple.vector_signature.iter().copied().sum::<f32>()
                / (icosuple.vector_signature.len() as f32)
        };
        Ok(CreateVertexResult {
            vertex_id: vertex_to_hex(&vertex_id),
            parents: parents.iter().map(vertex_to_hex).collect(),
            payload_bytes: icosuple.payload_bytes,
            entanglement: icosuple.entanglement_coefficient,
            ann_similarity: avg_similarity,
        })
    }

    fn prove_attribute(
        &mut self,
        params: ProveAttributeParams,
    ) -> OverlayResult<EntangledProofResult> {
        let manifold = params
            .manifold
            .clone()
            .unwrap_or_else(|| self.config.rpc.manifold_hint.clone());
        let witness_commitment = self.commit_witness(&params.did, &params.witness);
        let request = self.compose_request(&params)?;
        let started = Instant::now();
        let response = self.engine.handle_request(request)?;
        let elapsed_ms = started.elapsed().as_millis() as u64;
        let _ = self
            .telemetry
            .record_latency_ms("overlay.prove_attribute", elapsed_ms);
        let proof_id = hex::encode(response.dp_result.zk_proof_digest);
        let chsh = self.config.rpc.chsh_target + self.state.next_jitter();
        self.state.store_proof(
            proof_id.clone(),
            StoredProof {
                response: response.clone(),
                manifold: manifold.clone(),
                witness_commitment: witness_commitment.clone(),
                created_at: Instant::now(),
            },
        );
        self.broadcast_event(&OverlayFrame::ProofGenerated {
            proof_id: proof_id.clone(),
            did: params.did.clone(),
            attribute: params.attribute.clone(),
        })?;
        Ok(EntangledProofResult {
            proof_id,
            did: params.did,
            attribute: params.attribute,
            manifold,
            chsh_violation: chsh,
            witness_commitment,
            response,
        })
    }

    fn verify_proof(&mut self, params: VerifyProofParams) -> OverlayResult<VerifyProofResult> {
        let proof_id = params
            .proof_id
            .or_else(|| {
                params
                    .proof_object
                    .as_ref()
                    .and_then(|value| value.get("proof_id"))
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
            })
            .ok_or(OverlayError::MissingField("proof_id"))?;
        let stored = self.state.proof(&proof_id);
        let valid = stored.is_some();
        let vertex = stored.map(|proof| hex::encode(proof.response.enhanced_icosuple.vertex_id));
        let telemetry = if let (true, Some(proof)) = (valid && params.include_telemetry, stored) {
            Some(ProofTelemetry {
                verifier: self.config.node_id.clone(),
                verified_at_ms: SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or_default(),
                residual_budget: proof.response.dp_result.budget_claim.epsilon_remaining,
                global_latency_ms: proof.created_at.elapsed().as_millis() as u64,
            })
        } else {
            None
        };
        Ok(VerifyProofResult {
            proof_id,
            valid,
            vertex: params.vertex.or(vertex),
            telemetry,
        })
    }

    fn revoke_credential(
        &mut self,
        params: RevokeCredentialParams,
    ) -> OverlayResult<RevokeCredentialResult> {
        let mut hasher = Hasher::new();
        hasher.update(params.credential_id.as_bytes());
        hasher.update(&self.state.tick_lamport().to_le_bytes());
        let tx_hash = hex::encode(hasher.finalize().as_bytes());
        self.state
            .revocations
            .insert(params.credential_id.clone(), tx_hash.clone());
        self.broadcast_event(&OverlayFrame::CredentialRevoked {
            credential_id: params.credential_id.clone(),
            tx_hash: tx_hash.clone(),
        })?;
        Ok(RevokeCredentialResult {
            credential_id: params.credential_id,
            tx_hash,
        })
    }

    fn qtaid_tokenize(
        &mut self,
        params: QtaidTokenizeParams,
    ) -> OverlayResult<QtaidTokenizeResult> {
        let bits = params
            .bits_per_snp
            .unwrap_or(self.config.rpc.qtaid_bits_per_snp)
            .clamp(2, 4);
        let tokens = self.derive_qtaid_tokens(&params.genome_segment, bits);
        self.broadcast_event(&OverlayFrame::QtaidTokenized {
            owner_did: params.owner_did.clone(),
            token_count: tokens.len(),
        })?;
        Ok(QtaidTokenizeResult {
            owner_did: params.owner_did,
            allele_commitments: tokens,
            bits_per_snp: bits,
        })
    }

    fn compose_request(
        &mut self,
        params: &ProveAttributeParams,
    ) -> OverlayResult<PrivacyNetRequest> {
        let spatial = self.derive_spatial_domain(&params.witness);
        let epsilon = (params.attribute.len() as f64 / 50_000.0)
            .max(1e-6)
            .min(self.config.rpc.epsilon_cap);
        let mut query = DpQuery::gaussian(spatial, epsilon, DEFAULT_DELTA, 1.0);
        let session_id = self.state.next_session();
        query.composition_id = session_id;
        let fhe_slots = self.derive_fhe_slots(&params.witness);
        let parents = self.state.recent_parents(3);
        let lamport = self.state.tick_lamport();
        let chain_epoch = self.state.advance_epoch();
        Ok(PrivacyNetRequest {
            session_id,
            tenant_id: params.did.clone(),
            label: format!("entangled::{}", params.attribute),
            chain_epoch,
            dp_query: query,
            fhe_slots,
            parents,
            payload_bytes: params.witness.len() + params.attribute.len(),
            lamport,
            contribution_score: self.derive_contribution(&params.witness),
            ann_similarity: self.derive_similarity(&params.attribute),
            qrng_entropy_bits: 512,
            zk_claim: format!("{} proves {}", params.did, params.attribute),
            public_inputs: vec![params.attribute.clone(), params.did.clone()],
        })
    }

    fn broadcast_event(&self, frame: &OverlayFrame) -> OverlayResult<()> {
        let payload = serde_json::to_vec(frame)?;
        info!("overlay event={:?}", frame);
        let receipts = self.network.broadcast(&payload)?;
        let _ = self
            .telemetry
            .record_counter("overlay.broadcast_bytes", payload.len() as u64);
        let _ = self
            .telemetry
            .record_counter("overlay.broadcast_peers", receipts.len() as u64);
        Ok(())
    }

    fn derive_spatial_domain(&self, witness: &str) -> Vec<u64> {
        let digest = blake3::hash(witness.as_bytes());
        digest
            .as_bytes()
            .chunks(8)
            .take(4)
            .map(|chunk| {
                let mut buf = [0u8; 8];
                buf[..chunk.len()].copy_from_slice(chunk);
                u64::from_le_bytes(buf)
            })
            .collect()
    }

    fn derive_fhe_slots(&self, witness: &str) -> Vec<f64> {
        let digest = blake3::hash(format!("fhe::{witness}").as_bytes());
        digest
            .as_bytes()
            .chunks(4)
            .take(8)
            .map(|chunk| {
                let mut buf = [0u8; 4];
                buf[..chunk.len()].copy_from_slice(chunk);
                let raw = u32::from_le_bytes(buf) as f64 / u32::MAX as f64;
                (raw * 2.0) - 1.0
            })
            .collect()
    }

    fn derive_contribution(&self, witness: &str) -> f64 {
        let digest = blake3::hash(format!("score::{witness}").as_bytes());
        let raw = u64::from_le_bytes(digest.as_bytes()[..8].try_into().unwrap());
        0.1 + (raw as f64 / u64::MAX as f64) * 0.89
    }

    fn derive_similarity(&self, attribute: &str) -> f32 {
        let digest = blake3::hash(attribute.as_bytes());
        (digest.as_bytes()[0] as f32) / 255.0
    }

    fn commit_witness(&self, did: &str, witness: &str) -> String {
        let label = format!("{did}::{witness}");
        let hash = Blake3Hash::derive(label.as_bytes());
        hex::encode(hash.0)
    }

    fn derive_qtaid_tokens(&self, genome: &str, bits: u8) -> Vec<String> {
        genome
            .as_bytes()
            .chunks(3)
            .enumerate()
            .map(|(index, chunk)| {
                let mut seed = Vec::with_capacity(chunk.len() + 1);
                seed.extend_from_slice(chunk);
                seed.push(bits);
                let hash = blake3::hash(&seed);
                let mask = 0xFF >> (8 - bits);
                let allele = hash.as_bytes()[0] & mask;
                format!("qtaid:{bits}:{:02x}:{:04x}", allele, index)
            })
            .collect()
    }
}

#[allow(dead_code)]
struct StoredProof {
    response: PrivacyNetResponse,
    manifold: String,
    witness_commitment: String,
    created_at: Instant,
}

struct OverlayState {
    lamport: u64,
    epoch: u64,
    session_id: u64,
    rng: ChaCha20Rng,
    proofs: HashMap<String, StoredProof>,
    revocations: HashMap<String, String>,
    recent_vertices: VecDeque<VertexId>,
}

impl OverlayState {
    fn new(node_id: &str) -> Self {
        let mut hasher = Hasher::new();
        hasher.update(node_id.as_bytes());
        let mut seed = [0u8; 32];
        seed.copy_from_slice(hasher.finalize().as_bytes());
        Self {
            lamport: 1,
            epoch: 1,
            session_id: 1,
            rng: ChaCha20Rng::from_seed(seed),
            proofs: HashMap::new(),
            revocations: HashMap::new(),
            recent_vertices: VecDeque::new(),
        }
    }

    fn next_session(&mut self) -> u64 {
        let current = self.session_id;
        self.session_id = self.session_id.wrapping_add(1).max(1);
        current
    }

    fn tick_lamport(&mut self) -> u64 {
        self.lamport = self.lamport.wrapping_add(1).max(1);
        self.lamport
    }

    fn advance_epoch(&mut self) -> u64 {
        self.epoch = self.epoch.wrapping_add(1).max(1);
        self.epoch
    }

    fn next_jitter(&mut self) -> f32 {
        (self.rng.gen::<f32>() - 0.5) * 0.02
    }

    fn record_vertex(&mut self, vertex: VertexId) {
        if self.recent_vertices.len() >= RECENT_VERTEX_CAP {
            self.recent_vertices.pop_back();
        }
        self.recent_vertices.push_front(vertex);
    }

    fn recent_parents(&self, count: usize) -> Vec<VertexId> {
        self.recent_vertices.iter().take(count).copied().collect()
    }

    fn store_proof(&mut self, id: String, proof: StoredProof) {
        if self.proofs.len() >= MAX_PROOFS {
            if let Some(victim) = self.proofs.keys().next().cloned() {
                self.proofs.remove(&victim);
            }
        }
        self.proofs.insert(id, proof);
    }

    fn proof(&self, id: &str) -> Option<&StoredProof> {
        self.proofs.get(id)
    }
}
