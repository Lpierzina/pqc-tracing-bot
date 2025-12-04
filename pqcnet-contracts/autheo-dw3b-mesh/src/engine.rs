use std::collections::VecDeque;
use std::convert::TryInto;

use autheo_pqcnet_5dqeh::VertexId;
use autheo_privacynet::{
    dp::DpQuery,
    pipeline::{PrivacyNetEngine, PrivacyNetRequest, PrivacyNetResponse},
};
use serde::{Deserialize, Serialize};

use crate::{
    bloom::MeshBloomFilter,
    chaos::{ChaosObfuscator, ChaosTrajectory},
    compression::{CompressionPipeline, CompressionReport},
    config::Dw3bMeshConfig,
    entropy::{EntropySnapshot, QuantumEntropyPool},
    mesh::{AnonymityProof, MeshRoutePlan, MeshTopology},
    noise::{NoiseInjector, NoiseSummary},
    types::{MeshError, MeshResult},
};

pub struct Dw3bMeshEngine {
    config: Dw3bMeshConfig,
    privacy: PrivacyNetEngine,
    entropy: QuantumEntropyPool,
    noise: NoiseInjector,
    chaos: ChaosObfuscator,
    topology: MeshTopology,
    compressor: CompressionPipeline,
    state: MeshState,
}

impl Dw3bMeshEngine {
    pub fn new(config: Dw3bMeshConfig) -> Self {
        let mut entropy = QuantumEntropyPool::new(config.entropy.clone());
        let noise_seed = entropy.next_seed(b"dw3b-noise");
        let chaos_seed = entropy.next_seed(b"dw3b-chaos");
        let mut privacy_cfg = config.privacy.clone();
        privacy_cfg.ezph.zk_prover = config.zk_prover.clone();
        Self {
            topology: MeshTopology::new(config.mesh_weights.clone())
                .with_bloom(1 << 20, 0.01)
                .with_stake(10_000)
                .with_lambda(10.0),
            compressor: CompressionPipeline::new(11),
            privacy: PrivacyNetEngine::new(privacy_cfg),
            noise: NoiseInjector::new(config.primitives.clone(), noise_seed),
            chaos: ChaosObfuscator::new(chaos_seed),
            state: MeshState::default(),
            config,
            entropy,
        }
    }

    pub fn anonymize_query(
        &mut self,
        mut request: MeshAnonymizeRequest,
    ) -> MeshResult<MeshAnonymizeResponse> {
        if request.route_layers == 0 {
            return Err(MeshError::InvalidParameter(
                "route_layers must be greater than zero".into(),
            ));
        }
        let bloom_capacity = if request.bloom_capacity == 0 {
            1 << 20
        } else {
            request.bloom_capacity
        };
        let bloom_fp = if request.bloom_fp_rate <= 0.0 {
            0.01
        } else {
            request.bloom_fp_rate
        };
        let stake_threshold = if request.stake_threshold == 0 {
            10_000
        } else {
            request.stake_threshold
        };
        let lambda = if request.poisson_lambda <= 0.0 {
            10.0
        } else {
            request.poisson_lambda
        };
        self.topology = MeshTopology::new(self.config.mesh_weights.clone())
            .with_bloom(bloom_capacity, bloom_fp)
            .with_stake(stake_threshold)
            .with_lambda(lambda);
        request.bloom_capacity = bloom_capacity;
        request.bloom_fp_rate = bloom_fp;
        request.stake_threshold = stake_threshold;
        request.poisson_lambda = lambda;
        let session_id = self.state.next_session();
        let lamport = if let Some(hint) = request.lamport_hint.take() {
            self.state.override_lamport(hint)
        } else {
            self.state.tick_lamport()
        };
        let chain_epoch = self.state.advance_epoch();
        let parents = self.state.parents(4);
        let entropy_seed = self.entropy.next_seed(request.attribute.as_bytes());
        let mut route_plan = self.topology.plan_route(request.route_layers, entropy_seed);
        let mut bloom = MeshBloomFilter::new(request.bloom_capacity, request.bloom_fp_rate);
        bloom.insert(request.payload.as_bytes());
        bloom.insert(request.attribute.as_bytes());
        bloom.insert(request.did.as_bytes());
        for hop in &route_plan.hops {
            bloom.insert(&hop.stake_commitment);
        }
        route_plan.bloom_summary = bloom.summary();
        request.epsilon = request
            .epsilon
            .max(self.config.primitives.gaussian_epsilon)
            .min(1.0);
        request.delta = request
            .delta
            .max(self.config.primitives.gaussian_delta)
            .min(0.5);
        let priv_request =
            self.compose_privacynet_request(&request, session_id, lamport, chain_epoch, parents);
        let privacy_response = self.privacy.handle_request(priv_request)?;
        let vertex_bytes = privacy_response.enhanced_icosuple.vertex_id;
        self.state.record_vertex(VertexId(vertex_bytes));
        let chaos = self
            .chaos
            .sample(entropy_seed, request.route_layers as usize * 32);
        let noise = self.noise.inject(request.epsilon, request.delta);
        let entropy_snapshot = self.entropy.snapshot();
        let proof = self.topology.synthesize_proof(
            &privacy_response,
            &route_plan.bloom_summary,
            &noise,
            &chaos,
            &route_plan,
            &entropy_snapshot,
        );
        let payload = serde_json::to_vec(&privacy_response.enhanced_icosuple)?;
        let (compressed_payload, compression_report) = self.compressor.compress(&payload)?;
        Ok(MeshAnonymizeResponse {
            proof,
            route_plan,
            chaos,
            noise,
            compressed_payload,
            compression_report,
            privacy: privacy_response,
            entropy_snapshot,
        })
    }

    pub fn obfuscate_route(
        &mut self,
        data: &[u8],
        layers: u32,
        k_anonymity: f64,
    ) -> MeshResult<Vec<u8>> {
        let entropy_seed = self.entropy.next_seed(b"dw3b-obfuscate");
        let mut plan = self.topology.plan_route(layers, entropy_seed);
        plan.bloom_summary.fp_rate = (1.0 - k_anonymity).clamp(1e-6, 0.25);
        let mut buffer = data.to_vec();
        buffer.reverse();
        buffer.extend_from_slice(&plan.fingerprint());
        Ok(buffer)
    }

    pub fn entropy_beacon(&mut self, samples: u32, _five_d: bool) -> Vec<[u8; 512]> {
        self.entropy.vrbs(samples)
    }

    pub fn qtaid_prove(&mut self, request: QtaidProveRequest) -> MeshResult<QtaidProof> {
        let mesh_request = MeshAnonymizeRequest {
            attribute: format!("qtaid::{}", request.trait_name),
            payload: request.genome_segment.clone(),
            did: request.owner_did.clone(),
            epsilon: self.config.primitives.gaussian_epsilon,
            delta: self.config.primitives.gaussian_delta,
            route_layers: 3,
            bloom_capacity: request.genome_segment.len() as u64 * 4,
            bloom_fp_rate: 0.005,
            stake_threshold: (self.config.mesh_weights.stake.max(0.01) * 10_000.0) as u64,
            fhe_slots: vec![],
            public_inputs: vec![request.trait_name.clone()],
            lamport_hint: None,
            poisson_lambda: 10.0,
        };
        let response = self.anonymize_query(mesh_request)?;
        let tokens = derive_qtaid_tokens(&request.genome_segment, request.bits_per_snp);
        Ok(QtaidProof {
            tokens,
            bits_per_snp: request.bits_per_snp,
            response,
        })
    }

    fn compose_privacynet_request(
        &self,
        request: &MeshAnonymizeRequest,
        session_id: u64,
        lamport: u64,
        chain_epoch: u64,
        parents: Vec<VertexId>,
    ) -> PrivacyNetRequest {
        let spatial = derive_spatial_domain(&request.payload);
        let mut dp_query = DpQuery::gaussian(spatial, request.epsilon, request.delta, 1.0);
        dp_query.composition_id = session_id;
        let fhe_slots = if request.fhe_slots.is_empty() {
            derive_fhe_slots(&request.payload)
        } else {
            request.fhe_slots.clone()
        };
        let payload_bytes = request.payload.len() + request.attribute.len();
        PrivacyNetRequest {
            session_id,
            tenant_id: request.did.clone(),
            label: format!("dw3b::{}", request.attribute),
            chain_epoch,
            dp_query,
            fhe_slots,
            parents,
            payload_bytes,
            lamport,
            contribution_score: derive_contribution(&request.payload),
            ann_similarity: derive_similarity(&request.attribute),
            qrng_entropy_bits: self.config.entropy.vrb_size_bits,
            zk_claim: format!("{} proves {}", request.did, request.attribute),
            public_inputs: if request.public_inputs.is_empty() {
                vec![request.attribute.clone(), request.did.clone()]
            } else {
                request.public_inputs.clone()
            },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MeshAnonymizeRequest {
    pub did: String,
    pub attribute: String,
    pub payload: String,
    pub epsilon: f64,
    pub delta: f64,
    pub route_layers: u32,
    pub bloom_capacity: u64,
    pub bloom_fp_rate: f64,
    pub stake_threshold: u64,
    pub fhe_slots: Vec<f64>,
    pub public_inputs: Vec<String>,
    pub lamport_hint: Option<u64>,
    pub poisson_lambda: f64,
}

impl MeshAnonymizeRequest {
    pub fn demo() -> Self {
        Self {
            did: "did:autheo:demo".into(),
            attribute: "age > 18".into(),
            payload: "kyc-record-42".into(),
            epsilon: 1e-6,
            delta: 2f64.powi(-40),
            route_layers: 5,
            bloom_capacity: 1 << 18,
            bloom_fp_rate: 0.001,
            stake_threshold: 10_000,
            fhe_slots: vec![],
            public_inputs: vec![],
            lamport_hint: None,
            poisson_lambda: 10.0,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct MeshAnonymizeResponse {
    pub proof: AnonymityProof,
    pub route_plan: MeshRoutePlan,
    pub chaos: ChaosTrajectory,
    pub noise: NoiseSummary,
    pub compressed_payload: Vec<u8>,
    pub compression_report: CompressionReport,
    pub privacy: PrivacyNetResponse,
    pub entropy_snapshot: EntropySnapshot,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QtaidProveRequest {
    pub owner_did: String,
    pub trait_name: String,
    pub genome_segment: String,
    pub bits_per_snp: u8,
}

#[derive(Clone, Debug, Serialize)]
pub struct QtaidProof {
    pub tokens: Vec<String>,
    pub bits_per_snp: u8,
    pub response: MeshAnonymizeResponse,
}

#[derive(Default)]
struct MeshState {
    lamport: u64,
    epoch: u64,
    session_id: u64,
    parents: VecDeque<VertexId>,
}

impl MeshState {
    fn next_session(&mut self) -> u64 {
        self.session_id = self.session_id.wrapping_add(1).max(1);
        self.session_id
    }

    fn tick_lamport(&mut self) -> u64 {
        self.lamport = self.lamport.wrapping_add(1).max(1);
        self.lamport
    }

    fn override_lamport(&mut self, lamport: u64) -> u64 {
        self.lamport = lamport.max(1);
        self.lamport
    }

    fn advance_epoch(&mut self) -> u64 {
        self.epoch = self.epoch.wrapping_add(1).max(1);
        self.epoch
    }

    fn record_vertex(&mut self, vertex: VertexId) {
        if self.parents.len() >= 16 {
            self.parents.pop_back();
        }
        self.parents.push_front(vertex);
    }

    fn parents(&self, count: usize) -> Vec<VertexId> {
        self.parents.iter().take(count).copied().collect()
    }
}

fn derive_spatial_domain(payload: &str) -> Vec<u64> {
    let digest = blake3::hash(payload.as_bytes());
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

fn derive_fhe_slots(payload: &str) -> Vec<f64> {
    let digest = blake3::hash(format!("fhe::{payload}").as_bytes());
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

fn derive_contribution(payload: &str) -> f64 {
    let digest = blake3::hash(format!("score::{payload}").as_bytes());
    let raw = u64::from_le_bytes(digest.as_bytes()[..8].try_into().unwrap());
    0.1 + (raw as f64 / u64::MAX as f64) * 0.89
}

fn derive_similarity(attribute: &str) -> f32 {
    let digest = blake3::hash(attribute.as_bytes());
    (digest.as_bytes()[0] as f32) / 255.0
}

fn derive_qtaid_tokens(genome: &str, bits: u8) -> Vec<String> {
    genome
        .as_bytes()
        .chunks(3)
        .enumerate()
        .map(|(index, chunk)| {
            let mut seed = Vec::with_capacity(chunk.len() + 1);
            seed.extend_from_slice(chunk);
            seed.push(bits);
            let hash = blake3::hash(&seed);
            let mask = 0xFF >> (8 - bits.min(6));
            let allele = hash.as_bytes()[0] & mask;
            format!("qtaid:{bits}:{:02x}:{:04x}", allele, index)
        })
        .collect()
}
