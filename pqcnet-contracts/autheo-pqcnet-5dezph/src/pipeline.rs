use autheo_pqcnet_5dqeh::{
    CrystallineVoxel, HypergraphModule, Icosuple, MsgAnchorEdge, PqcBinding, PqcLayer, PqcScheme,
    PulsedLaserLink, QuantumCoordinates, VertexId,
};
use blake3::{hash, Hasher};
use serde::{Deserialize, Serialize};

use crate::{
    chaos::{ChaosEngine, LorenzChuaChaos},
    config::{EzphConfig, FheBackendKind, ZkConfig, ZkProverKind},
    error::{EzphError, EzphResult},
    fhe::{FheCiphertext, FheEvaluator, MockCkksEvaluator, TfheCkksEvaluator},
    manifold::{project_dimensions, DimensionProjection, EzphManifoldState},
    privacy::{evaluate_privacy, EzphPrivacyReport},
    zk::{Halo2ZkProver, MockCircomProver, ZkProof, ZkProver, ZkStatement},
};

pub enum DefaultEzphPipeline {
    Production(EzphPipeline<Halo2ZkProver, TfheCkksEvaluator, LorenzChuaChaos>),
    Mock(EzphPipeline<MockCircomProver, MockCkksEvaluator, LorenzChuaChaos>),
}

pub type MockEzphPipeline = EzphPipeline<MockCircomProver, MockCkksEvaluator, LorenzChuaChaos>;
pub type ProductionEzphPipeline = EzphPipeline<Halo2ZkProver, TfheCkksEvaluator, LorenzChuaChaos>;

impl DefaultEzphPipeline {
    pub fn new(config: EzphConfig) -> Self {
        if config.zk_prover == ZkProverKind::MockCircom
            || config.fhe_evaluator == FheBackendKind::MockCkks
        {
            Self::Mock(EzphPipeline::mock(config))
        } else {
            Self::Production(EzphPipeline::production(config))
        }
    }

    pub fn entangle_and_anchor(
        &self,
        module: &mut HypergraphModule,
        request: EzphRequest,
    ) -> EzphResult<EzphOutcome> {
        match self {
            Self::Production(pipeline) => pipeline.entangle_and_anchor(module, request),
            Self::Mock(pipeline) => pipeline.entangle_and_anchor(module, request),
        }
    }

    pub fn config(&self) -> &EzphConfig {
        match self {
            Self::Production(pipeline) => &pipeline.config,
            Self::Mock(pipeline) => &pipeline.config,
        }
    }
}

pub struct EzphPipeline<P, F, C> {
    pub(crate) config: EzphConfig,
    zk: P,
    fhe: F,
    chaos: C,
}

impl EzphPipeline<MockCircomProver, MockCkksEvaluator, LorenzChuaChaos> {
    pub fn new(config: EzphConfig) -> Self {
        Self::mock(config)
    }

    pub fn mock(config: EzphConfig) -> Self {
        let zk = MockCircomProver::new(config.zk.clone());
        let fhe = MockCkksEvaluator::new(config.fhe.clone());
        let chaos = LorenzChuaChaos::new(config.chaos.clone());
        Self {
            config,
            zk,
            fhe,
            chaos,
        }
    }
}

impl EzphPipeline<Halo2ZkProver, TfheCkksEvaluator, LorenzChuaChaos> {
    pub fn production(config: EzphConfig) -> Self {
        let zk = Halo2ZkProver::new(config.zk.clone())
            .unwrap_or_else(|err| panic!("failed to initialize Halo2 prover: {err}"));
        let fhe = TfheCkksEvaluator::new(config.fhe.clone());
        let chaos = LorenzChuaChaos::new(config.chaos.clone());
        Self {
            config,
            zk,
            fhe,
            chaos,
        }
    }
}

impl<P, F, C> EzphPipeline<P, F, C>
where
    P: ZkProver,
    F: FheEvaluator,
    C: ChaosEngine,
{
    pub fn entangle_and_anchor(
        &self,
        module: &mut HypergraphModule,
        request: EzphRequest,
    ) -> EzphResult<EzphOutcome> {
        let seed = derive_seed(&request);
        let chaos = self.chaos.sample(&seed);
        let manifold =
            EzphManifoldState::build(&self.config.manifold, chaos, &seed, &request.fhe_slots);
        let ciphertext = self.fhe.encrypt(&request.fhe_slots)?;
        let statement = request.statement(&self.config.zk);
        let zk_proof = self.zk.prove(&statement)?;
        let icosuple = build_icosuple(
            &self.config,
            &manifold,
            &ciphertext,
            &zk_proof,
            &seed,
            &request,
        );
        let msg = request.into_msg_anchor(icosuple, self.config.qeh.max_parent_links);
        let receipt = module.apply_anchor_edge(msg)?;
        let privacy = evaluate_privacy(&manifold, &ciphertext, &self.config.privacy);
        if !privacy.satisfied {
            return Err(EzphError::from(privacy));
        }
        let projections = project_dimensions(&manifold, self.config.manifold.projection_rank)
            .map_err(|rank| EzphError::InvalidProjectionRank { rank })?;
        Ok(EzphOutcome {
            receipt,
            privacy,
            zk_proof,
            fhe_ciphertext: ciphertext,
            projections,
            manifold,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EzphRequest {
    pub label: String,
    pub chain_epoch: u64,
    pub parents: Vec<VertexId>,
    pub payload_bytes: usize,
    pub lamport: u64,
    pub contribution_score: f64,
    pub ann_similarity: f32,
    pub qrng_entropy_bits: u16,
    pub pqc_binding: PqcBinding,
    pub autheo_identity: String,
    pub tuple_commitment: [u8; 32],
    pub zk_claim: String,
    pub public_inputs: Vec<String>,
    pub fhe_slots: Vec<f64>,
    pub parent_coherence_hint: Option<f64>,
}

impl EzphRequest {
    pub fn statement(&self, config: &ZkConfig) -> ZkStatement {
        ZkStatement {
            circuit_id: config.circuit_id.clone(),
            claim: self.zk_claim.clone(),
            public_inputs: self.public_inputs.clone(),
        }
    }

    pub fn parent_coherence(&self, max_parents: usize) -> f64 {
        self.parent_coherence_hint.unwrap_or_else(|| {
            if self.parents.is_empty() {
                0.1
            } else {
                (self.parents.len() as f64 / max_parents as f64).min(1.0)
            }
        })
    }

    pub fn into_msg_anchor(self, icosuple: Icosuple, max_parent_links: usize) -> MsgAnchorEdge {
        let parent_coherence = self.parent_coherence(max_parent_links);
        MsgAnchorEdge {
            request_id: derive_request_id(&self),
            chain_epoch: self.chain_epoch,
            parents: self.parents,
            parent_coherence,
            lamport: self.lamport,
            contribution_score: self.contribution_score,
            ann_similarity: self.ann_similarity,
            qrng_entropy_bits: self.qrng_entropy_bits,
            pqc_binding: self.pqc_binding,
            icosuple,
        }
    }

    pub fn demo(label: &str) -> Self {
        let mut commitment = [0u8; 32];
        commitment.copy_from_slice(hash(label.as_bytes()).as_bytes());
        Self {
            label: label.into(),
            chain_epoch: 0,
            parents: vec![],
            payload_bytes: 2_048,
            lamport: 1,
            contribution_score: 0.6,
            ann_similarity: 0.88,
            qrng_entropy_bits: 512,
            pqc_binding: PqcBinding::simulated(label),
            autheo_identity: format!("did:autheo:{label}"),
            tuple_commitment: commitment,
            zk_claim: "age >= 18".into(),
            public_inputs: vec!["attr:age".into(), "bound:18".into()],
            fhe_slots: vec![0.25, 0.5, 0.125, 0.625],
            parent_coherence_hint: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct EzphOutcome {
    pub receipt: autheo_pqcnet_5dqeh::VertexReceipt,
    pub privacy: EzphPrivacyReport,
    pub zk_proof: ZkProof,
    pub fhe_ciphertext: FheCiphertext,
    pub projections: Vec<DimensionProjection>,
    pub manifold: EzphManifoldState,
}

fn derive_seed(request: &EzphRequest) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(request.label.as_bytes());
    hasher.update(&request.lamport.to_le_bytes());
    hasher.update(&request.chain_epoch.to_le_bytes());
    hasher.update(&request.tuple_commitment);
    for slot in &request.fhe_slots {
        hasher.update(&slot.to_le_bytes());
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(hasher.finalize().as_bytes());
    out
}

fn derive_request_id(request: &EzphRequest) -> u64 {
    let mut hasher = Hasher::new();
    hasher.update(request.label.as_bytes());
    hasher.update(request.autheo_identity.as_bytes());
    hasher.update(&request.chain_epoch.to_le_bytes());
    hasher.update(&request.lamport.to_le_bytes());
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&hasher.finalize().as_bytes()[..8]);
    u64::from_le_bytes(bytes)
}

fn build_icosuple(
    config: &EzphConfig,
    manifold: &EzphManifoldState,
    ciphertext: &FheCiphertext,
    proof: &ZkProof,
    seed: &[u8; 32],
    request: &EzphRequest,
) -> Icosuple {
    let mut vector_signature = Vec::with_capacity(config.qeh.vector_dimensions);
    let mut hasher = Hasher::new();
    hasher.update(seed);
    hasher.update(&ciphertext.digest);
    hasher.update(&proof.statement_hash);
    let mut reader = hasher.finalize_xof();
    for _ in 0..config.qeh.vector_dimensions {
        let mut buf = [0u8; 4];
        reader.fill(&mut buf);
        let raw = u32::from_le_bytes(buf);
        let value = (raw as f64 / u32::MAX as f64) as f32;
        vector_signature.push(value);
    }
    let entanglement = manifold.homomorphic_amplitude.tanh().abs() as f32;
    let tau = std::f64::consts::PI * 2.0;
    let quantum_coordinates = QuantumCoordinates::new(
        manifold.spatial[0],
        manifold.spatial[1],
        manifold.spatial[2],
        manifold.temporal_noise,
        manifold.chaos.phase() * tau,
    );
    let crystalline_voxel = CrystallineVoxel::new(
        manifold.spatial[0] * 1.1,
        manifold.spatial[1] * 1.1,
        manifold.spatial[2] * 1.1,
        ciphertext.scale,
        manifold.chaos.phase(),
    );
    let channel_divisor = config.qeh.laser_channels.max(1) as usize;
    let channel_id = (request.parents.len() % channel_divisor) as u16;
    let laser_link = PulsedLaserLink::new(
        channel_id,
        config.qeh.laser_throughput_gbps,
        config.qeh.laser_latency_ps,
        true,
    );
    let pqc_layers = vec![
        PqcLayer {
            scheme: PqcScheme::Kyber,
            metadata_tag: "kyber-kem".into(),
            epoch: request.chain_epoch,
        },
        PqcLayer {
            scheme: PqcScheme::Dilithium,
            metadata_tag: "dilithium-sig".into(),
            epoch: request.chain_epoch,
        },
        PqcLayer {
            scheme: PqcScheme::Hybrid("ezph-zk".into()),
            metadata_tag: format!("zk::{}", proof.proof_system),
            epoch: request.chain_epoch,
        },
    ];
    Icosuple {
        label: request.label.clone(),
        payload_bytes: request.payload_bytes,
        pqc_layers,
        vector_signature,
        quantum_coordinates,
        entanglement_coefficient: entanglement,
        crystalline_voxel,
        laser_link,
    }
}
