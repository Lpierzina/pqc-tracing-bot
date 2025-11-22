//! 5D-QEH (Five-Dimensional Qubit-Enhanced Hypergraph) module primitives for Autheo PQCNet.
//!
//! The crate now targets chain-module embedding first and simulations second. It defines the
//! deterministic hypergraph state machine, storage layout helpers (hot vs crystalline), and an
//! `HypergraphModule` facade that can be dropped into the Autheo node runtime or compiled to
//! `wasm32-unknown-unknown`. The legacy simulator still exists inside `examples/` so architects
//! can benchmark epochs, but the primary API is now the module entry points (`MsgAnchorEdge`,
//! `HypergraphModule::apply_anchor_edge`, and PQC bindings backed by `autheo-pqc-core`).

use blake3::Hasher;
use core::f64::consts::PI;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};

mod entropy;
mod pqc;
#[cfg(feature = "sim")]
pub use entropy::QrngEntropyRng;
pub use pqc::{
    CorePqcRuntime, PqcHandshakeReceipt, PqcHandshakeRequest, PqcRotationOutcome, PqcRuntime,
    PqcRuntimeError, PqcSignature,
};
pub use pqcnet_entropy::{EntropyError, EntropySource, HostEntropySource};

/// Canonical byte size for 5D-QEH icosuples.
pub const ICOSUPLE_BYTES: usize = 4096;

/// Describes how many parents a vertex is allowed to entangle with.
pub const MAX_PARENT_LINKS: usize = 100;

/// High-level configuration shared by the hypergraph state machine and simulator.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct QehConfig {
    pub max_parent_links: usize,
    pub ann_similarity_threshold: f32,
    pub crystalline_offload_after_ms: u64,
    pub crystalline_payload_threshold: usize,
    pub laser_channels: u16,
    pub vector_dimensions: usize,
    pub vector_similarity_floor: f32,
    pub quantum_coordinate_scale_mm: f64,
    pub temporal_precision_ps: f64,
    pub crystalline_density_tb_per_cm3: f64,
    pub laser_latency_ps: f64,
    pub laser_throughput_gbps: f64,
}

impl Default for QehConfig {
    fn default() -> Self {
        Self {
            max_parent_links: MAX_PARENT_LINKS,
            ann_similarity_threshold: 0.78,
            crystalline_offload_after_ms: 2_592_000_000, // 30 days in milliseconds
            crystalline_payload_threshold: 3_584,
            laser_channels: 16,
            vector_dimensions: 2_048,
            vector_similarity_floor: 0.8,
            quantum_coordinate_scale_mm: 5.0,
            temporal_precision_ps: 1.0,
            crystalline_density_tb_per_cm3: 360.0,
            laser_latency_ps: 0.75,
            laser_throughput_gbps: 1_000_000.0,
        }
    }
}

/// Five-dimensional coordinates for a vertex (x, y, z, temporal, quantum phase).
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct QuantumCoordinates {
    pub x_mm: f64,
    pub y_mm: f64,
    pub z_mm: f64,
    pub temporal_ps: f64,
    pub phase_radians: f64,
}

impl QuantumCoordinates {
    pub const fn new(
        x_mm: f64,
        y_mm: f64,
        z_mm: f64,
        temporal_ps: f64,
        phase_radians: f64,
    ) -> Self {
        Self {
            x_mm,
            y_mm,
            z_mm,
            temporal_ps,
            phase_radians,
        }
    }

    fn hash_into(&self, hasher: &mut Hasher) {
        hasher.update(&self.x_mm.to_le_bytes());
        hasher.update(&self.y_mm.to_le_bytes());
        hasher.update(&self.z_mm.to_le_bytes());
        hasher.update(&self.temporal_ps.to_le_bytes());
        hasher.update(&self.phase_radians.to_le_bytes());
    }
}

/// Encodes how an icosuple is mapped into crystalline storage.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct CrystallineVoxel {
    pub x_mm: f64,
    pub y_mm: f64,
    pub z_mm: f64,
    pub intensity_bits: f64,
    pub polarization_radians: f64,
}

impl CrystallineVoxel {
    pub const fn new(
        x_mm: f64,
        y_mm: f64,
        z_mm: f64,
        intensity_bits: f64,
        polarization_radians: f64,
    ) -> Self {
        Self {
            x_mm,
            y_mm,
            z_mm,
            intensity_bits,
            polarization_radians,
        }
    }

    fn hash_into(&self, hasher: &mut Hasher) {
        hasher.update(&self.x_mm.to_le_bytes());
        hasher.update(&self.y_mm.to_le_bytes());
        hasher.update(&self.z_mm.to_le_bytes());
        hasher.update(&self.intensity_bits.to_le_bytes());
        hasher.update(&self.polarization_radians.to_le_bytes());
    }
}

/// Pulsed laser telemetry attached to each anchored vertex.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct PulsedLaserLink {
    pub channel_id: u16,
    pub throughput_gbps: f64,
    pub latency_ps: f64,
    pub qkd_active: bool,
}

impl PulsedLaserLink {
    pub const fn new(
        channel_id: u16,
        throughput_gbps: f64,
        latency_ps: f64,
        qkd_active: bool,
    ) -> Self {
        Self {
            channel_id,
            throughput_gbps,
            latency_ps,
            qkd_active,
        }
    }

    fn hash_into(&self, hasher: &mut Hasher) {
        hasher.update(&self.channel_id.to_le_bytes());
        hasher.update(&self.throughput_gbps.to_le_bytes());
        hasher.update(&self.latency_ps.to_le_bytes());
        hasher.update(&[self.qkd_active as u8]);
    }
}

/// Temporal weight coefficients that approximate TW-weighted voting in 5D-QEH.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TemporalWeightModel {
    lamport_gain: f64,
    coherence_gain: f64,
    entropy_gain: f64,
    contribution_gain: f64,
    ann_gain: f64,
    entanglement_gain: f64,
}

impl TemporalWeightModel {
    pub const fn new(
        lamport_gain: f64,
        coherence_gain: f64,
        entropy_gain: f64,
        contribution_gain: f64,
        ann_gain: f64,
        entanglement_gain: f64,
    ) -> Self {
        Self {
            lamport_gain,
            coherence_gain,
            entropy_gain,
            contribution_gain,
            ann_gain,
            entanglement_gain,
        }
    }

    /// Evaluate the temporal weight given the observed entanglement metrics.
    pub fn score(&self, input: &TemporalWeightInput) -> f64 {
        let lamport_term = ((input.lamport as f64 / 1_000.0) + 1.0).ln() * self.lamport_gain;
        let coherence_term = input.parent_coherence.clamp(0.0, 1.0) * self.coherence_gain;
        let entropy_term = ((input.qrng_entropy_bits as f64) / 512.0).min(1.0) * self.entropy_gain;
        let contribution_term = input.contribution_score * self.contribution_gain;
        let ann_term = input.ann_similarity as f64 * self.ann_gain;
        let entanglement_term =
            input.entanglement_coefficient.clamp(0.0, 1.0) * self.entanglement_gain;
        (lamport_term
            + coherence_term
            + entropy_term
            + contribution_term
            + ann_term
            + entanglement_term)
            .clamp(0.0, 10.0)
    }
}

impl Default for TemporalWeightModel {
    fn default() -> Self {
        Self::new(0.65, 2.1, 1.3, 0.9, 1.4, 1.8)
    }
}

/// Inputs provided to the temporal-weight model when inserting a vertex.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TemporalWeightInput {
    pub lamport: u64,
    pub parent_coherence: f64,
    pub qrng_entropy_bits: u16,
    pub contribution_score: f64,
    pub ann_similarity: f32,
    pub entanglement_coefficient: f64,
}

impl TemporalWeightInput {
    pub fn new(
        lamport: u64,
        parent_coherence: f64,
        qrng_entropy_bits: u16,
        contribution_score: f64,
        ann_similarity: f32,
        entanglement_coefficient: f64,
    ) -> Self {
        Self {
            lamport,
            parent_coherence,
            qrng_entropy_bits,
            contribution_score,
            ann_similarity,
            entanglement_coefficient,
        }
    }
}

/// Compact identifier for hypergraph vertices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct VertexId(pub [u8; 32]);

impl VertexId {
    pub fn random<R: EntropySource>(rng: &mut R) -> Self {
        let mut bytes = [0u8; 32];
        rng.fill_bytes(&mut bytes);
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl core::fmt::Display for VertexId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// Supported PQC primitives embedded into an icosuple.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PqcScheme {
    Kyber,
    Dilithium,
    Falcon,
    Hybrid(String),
}

/// Metadata associated with a PQC layer.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PqcLayer {
    pub scheme: PqcScheme,
    pub metadata_tag: String,
    pub epoch: u64,
}

/// Logical representation of a 4096-byte icosuple.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Icosuple {
    pub label: String,
    pub payload_bytes: usize,
    pub pqc_layers: Vec<PqcLayer>,
    pub vector_signature: Vec<f32>,
    pub quantum_coordinates: QuantumCoordinates,
    pub entanglement_coefficient: f32,
    pub crystalline_voxel: CrystallineVoxel,
    pub laser_link: PulsedLaserLink,
}

impl Icosuple {
    /// Synthesizes an icosuple for demos/simulations.
    pub fn synthesize(
        config: &QehConfig,
        label: impl Into<String>,
        payload_bytes: usize,
        similarity_hint: f32,
    ) -> Self {
        let label = label.into();
        let dims = config.vector_dimensions.max(1);
        let normalized = similarity_hint.clamp(0.0, 1.0);
        let mut vector_signature = Vec::with_capacity(dims);
        for i in 0..dims {
            let phase = ((i as f32) * 0.37).sin().abs();
            let blended = ((phase * 0.5) + normalized).min(1.0);
            vector_signature.push(blended);
        }

        let ent_seed = derive_seed(&label, payload_bytes, dims, normalized, b"entangle");
        let entanglement_coefficient = derive_entanglement(ent_seed, normalized);
        let quantum_seed = derive_seed(&label, payload_bytes, dims, normalized, b"quantum");
        let quantum_coordinates = build_quantum_coordinates(&quantum_seed, config);
        let voxel_seed = derive_seed(&label, payload_bytes, dims, normalized, b"voxel");
        let crystalline_voxel = build_crystalline_voxel(&voxel_seed, config);
        let laser_seed = derive_seed(&label, payload_bytes, dims, normalized, b"laser");
        let laser_link = build_laser_link(&laser_seed, config);

        let pqc_layers = vec![
            PqcLayer {
                scheme: PqcScheme::Kyber,
                metadata_tag: "kyber-kem".into(),
                epoch: 0,
            },
            PqcLayer {
                scheme: PqcScheme::Dilithium,
                metadata_tag: "dilithium-sig".into(),
                epoch: 0,
            },
        ];

        Self {
            label,
            payload_bytes,
            pqc_layers,
            vector_signature,
            quantum_coordinates,
            entanglement_coefficient,
            crystalline_voxel,
            laser_link,
        }
    }

    pub fn vertex_id(&self, parents: &[VertexId]) -> VertexId {
        let mut hasher = Hasher::new();
        hasher.update(self.label.as_bytes());
        hasher.update(&self.payload_bytes.to_le_bytes());
        for layer in &self.pqc_layers {
            let marker = match &layer.scheme {
                PqcScheme::Kyber => b"kyber".as_slice(),
                PqcScheme::Dilithium => b"dilithium".as_slice(),
                PqcScheme::Falcon => b"falcon".as_slice(),
                PqcScheme::Hybrid(name) => name.as_bytes(),
            };
            hasher.update(marker);
            hasher.update(layer.metadata_tag.as_bytes());
            hasher.update(&layer.epoch.to_le_bytes());
        }
        for value in &self.vector_signature {
            hasher.update(&value.to_le_bytes());
        }
        for parent in parents {
            hasher.update(parent.as_bytes());
        }
        self.quantum_coordinates.hash_into(&mut hasher);
        hasher.update(&self.entanglement_coefficient.to_le_bytes());
        self.crystalline_voxel.hash_into(&mut hasher);
        self.laser_link.hash_into(&mut hasher);
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(hasher.finalize().as_bytes());
        VertexId(bytes)
    }
}

const TAU: f64 = PI * 2.0;
const BITS_PER_TB: f64 = 8.0 * 1_000_000_000_000.0;

fn derive_seed(
    label: &str,
    payload_bytes: usize,
    dims: usize,
    similarity: f32,
    salt: &[u8],
) -> [u8; 64] {
    let mut hasher = Hasher::new();
    hasher.update(label.as_bytes());
    hasher.update(&payload_bytes.to_le_bytes());
    hasher.update(&(dims as u64).to_le_bytes());
    hasher.update(&similarity.to_le_bytes());
    hasher.update(salt);
    let mut reader = hasher.finalize_xof();
    let mut seed = [0u8; 64];
    reader.fill(&mut seed);
    seed
}

fn next_unit(seed: &[u8; 64], cursor: &mut usize) -> f64 {
    if *cursor + 8 > seed.len() {
        *cursor = 0;
    }
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&seed[*cursor..*cursor + 8]);
    *cursor += 8;
    let raw = u64::from_le_bytes(bytes);
    (raw as f64) / (u64::MAX as f64)
}

fn range_value(unit: f64, min: f64, max: f64) -> f64 {
    min + (max - min) * unit
}

fn derive_entanglement(seed: [u8; 64], similarity: f32) -> f32 {
    let mut cursor = 0;
    let noise = next_unit(&seed, &mut cursor);
    (((similarity as f64) * 0.65) + noise * 0.35).clamp(0.0, 1.0) as f32
}

fn build_quantum_coordinates(seed: &[u8; 64], config: &QehConfig) -> QuantumCoordinates {
    let mut cursor = 0;
    let min = -config.quantum_coordinate_scale_mm;
    let max = config.quantum_coordinate_scale_mm;
    let x = range_value(next_unit(seed, &mut cursor), min, max);
    let y = range_value(next_unit(seed, &mut cursor), min, max);
    let z = range_value(next_unit(seed, &mut cursor), min, max);
    let temporal = range_value(
        next_unit(seed, &mut cursor),
        0.0,
        config.temporal_precision_ps.max(0.0),
    );
    let phase = range_value(next_unit(seed, &mut cursor), 0.0, TAU);
    QuantumCoordinates::new(x, y, z, temporal, phase)
}

fn build_crystalline_voxel(seed: &[u8; 64], config: &QehConfig) -> CrystallineVoxel {
    let mut cursor = 0;
    let min = -config.quantum_coordinate_scale_mm * 1.2;
    let max = config.quantum_coordinate_scale_mm * 1.2;
    let x = range_value(next_unit(seed, &mut cursor), min, max);
    let y = range_value(next_unit(seed, &mut cursor), min, max);
    let z = range_value(next_unit(seed, &mut cursor), min, max);
    let density_bits = config.crystalline_density_tb_per_cm3.max(0.0) * BITS_PER_TB;
    let intensity = range_value(
        next_unit(seed, &mut cursor),
        0.5 * density_bits,
        density_bits,
    );
    let polarization = range_value(next_unit(seed, &mut cursor), 0.0, TAU);
    CrystallineVoxel::new(x, y, z, intensity, polarization)
}

fn build_laser_link(seed: &[u8; 64], config: &QehConfig) -> PulsedLaserLink {
    let mut cursor = 0;
    let channels = config.laser_channels.max(1);
    let channel_unit = next_unit(seed, &mut cursor);
    let mut channel_id = (channel_unit * channels as f64)
        .floor()
        .min((channels - 1) as f64) as u16;
    if config.laser_channels == 0 {
        channel_id = 0;
    }
    let throughput_scale = range_value(next_unit(seed, &mut cursor), 0.9, 1.1);
    let latency_scale = range_value(next_unit(seed, &mut cursor), 0.25, 0.9);
    let qkd_active = next_unit(seed, &mut cursor) > 0.2;
    PulsedLaserLink::new(
        channel_id,
        config.laser_throughput_gbps * throughput_scale.max(0.1),
        config.laser_latency_ps * latency_scale.max(0.1),
        qkd_active,
    )
}

/// Errors emitted when inserting or simulating vertices.
#[derive(thiserror::Error, Debug, PartialEq)]
pub enum HypergraphError {
    #[error("icosuple payload {payload}B exceeds limit of {limit}B")]
    PayloadTooLarge { payload: usize, limit: usize },
    #[error("icosuple references {given} parents but limit is {limit}")]
    TooManyParents { given: usize, limit: usize },
    #[error("vector embedding has {given} dimensions but expected {expected}")]
    EmbeddingMismatch { given: usize, expected: usize },
    #[error("entanglement coefficient {coefficient} outside 0..=1")]
    InvalidEntanglementCoefficient { coefficient: f32 },
}

/// Storage placement for an icosuple.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StorageTarget {
    Hot,
    Crystalline,
}

/// Receipt returned after inserting a vertex.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VertexReceipt {
    pub vertex_id: VertexId,
    pub tw_score: f64,
    pub storage: StorageTarget,
    pub ann_similarity: f32,
    pub parents: usize,
    pub quantum_coordinates: QuantumCoordinates,
    pub entanglement_coefficient: f32,
    pub crystalline_voxel: CrystallineVoxel,
    pub laser_link: PulsedLaserLink,
    pub pqc_signature: Option<PqcSignature>,
}

/// Materialized vertex information.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HyperVertex {
    pub receipt: VertexReceipt,
    pub label: String,
    pub payload_bytes: usize,
    pub parents: Vec<VertexId>,
    pub pqc_layers: Vec<PqcLayer>,
    pub embedding: Vec<f32>,
}

/// Deterministic hypergraph state machine.
pub struct HypergraphState {
    config: QehConfig,
    vertices: BTreeMap<VertexId, HyperVertex>,
}

impl HypergraphState {
    pub fn new(config: QehConfig) -> Self {
        Self {
            config,
            vertices: BTreeMap::new(),
        }
    }

    pub fn config(&self) -> &QehConfig {
        &self.config
    }

    pub fn len(&self) -> usize {
        self.vertices.len()
    }

    pub fn get(&self, id: &VertexId) -> Option<&HyperVertex> {
        self.vertices.get(id)
    }

    pub fn insert(
        &mut self,
        icosuple: Icosuple,
        parents: Vec<VertexId>,
        model: &TemporalWeightModel,
        tw_input: TemporalWeightInput,
        pqc_signature: Option<PqcSignature>,
    ) -> Result<VertexReceipt, HypergraphError> {
        if icosuple.payload_bytes > ICOSUPLE_BYTES {
            return Err(HypergraphError::PayloadTooLarge {
                payload: icosuple.payload_bytes,
                limit: ICOSUPLE_BYTES,
            });
        }
        if icosuple.vector_signature.len() != self.config.vector_dimensions {
            return Err(HypergraphError::EmbeddingMismatch {
                given: icosuple.vector_signature.len(),
                expected: self.config.vector_dimensions,
            });
        }
        if !(0.0..=1.0).contains(&icosuple.entanglement_coefficient) {
            return Err(HypergraphError::InvalidEntanglementCoefficient {
                coefficient: icosuple.entanglement_coefficient,
            });
        }
        if parents.len() > self.config.max_parent_links {
            return Err(HypergraphError::TooManyParents {
                given: parents.len(),
                limit: self.config.max_parent_links,
            });
        }

        let vertex_id = icosuple.vertex_id(&parents);
        let tw_score = model.score(&tw_input);
        let storage = if self.should_archive(&icosuple, &tw_input) {
            StorageTarget::Crystalline
        } else {
            StorageTarget::Hot
        };

        let receipt = VertexReceipt {
            vertex_id,
            tw_score,
            storage: storage.clone(),
            ann_similarity: tw_input.ann_similarity,
            parents: parents.len(),
            quantum_coordinates: icosuple.quantum_coordinates,
            entanglement_coefficient: icosuple.entanglement_coefficient,
            crystalline_voxel: icosuple.crystalline_voxel,
            laser_link: icosuple.laser_link,
            pqc_signature: pqc_signature.clone(),
        };

        let vertex = HyperVertex {
            receipt: receipt.clone(),
            label: icosuple.label,
            payload_bytes: icosuple.payload_bytes,
            parents,
            pqc_layers: icosuple.pqc_layers,
            embedding: icosuple.vector_signature,
        };

        self.vertices.insert(receipt.vertex_id, vertex);
        Ok(receipt)
    }

    fn should_archive(&self, icosuple: &Icosuple, tw_input: &TemporalWeightInput) -> bool {
        tw_input.lamport >= self.config.crystalline_offload_after_ms
            || icosuple.payload_bytes >= self.config.crystalline_payload_threshold
            || tw_input.ann_similarity < self.config.ann_similarity_threshold
            || icosuple.entanglement_coefficient < self.config.vector_similarity_floor
    }
}

/// Storage counters exposed to the runtime so it can persist hot vs crystalline sets.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModuleStorageLayout {
    pub hot_vertices: usize,
    pub crystalline_vertices: usize,
}

impl ModuleStorageLayout {
    pub fn register(&mut self, receipt: &VertexReceipt) {
        match receipt.storage {
            StorageTarget::Hot => self.hot_vertices += 1,
            StorageTarget::Crystalline => self.crystalline_vertices += 1,
        }
    }

    pub fn total_vertices(&self) -> usize {
        self.hot_vertices + self.crystalline_vertices
    }
}

/// Associates a hypergraph invocation with a PQC key / engine slot.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PqcBinding {
    pub key_id: String,
    pub scheme: PqcScheme,
}

impl PqcBinding {
    pub fn new(key_id: impl Into<String>, scheme: PqcScheme) -> Self {
        Self {
            key_id: key_id.into(),
            scheme,
        }
    }

    pub fn simulated(label: impl Into<String>) -> Self {
        Self::new(label, PqcScheme::Kyber)
    }

    pub fn request_handshake<R: PqcRuntime + ?Sized>(
        &self,
        runtime: &R,
        request: &PqcHandshakeRequest,
    ) -> Result<PqcHandshakeReceipt, PqcRuntimeError> {
        runtime.pqc_handshake(self, request)
    }

    pub fn request_signature<R: PqcRuntime + ?Sized>(
        &self,
        runtime: &R,
        payload: &[u8],
    ) -> Result<PqcSignature, PqcRuntimeError> {
        runtime.pqc_sign(self, payload)
    }

    pub fn rotate_if_needed<R: PqcRuntime + ?Sized>(
        &self,
        runtime: &R,
        now_ms: u64,
    ) -> Result<PqcRotationOutcome, PqcRuntimeError> {
        runtime.pqc_rotate(self, now_ms)
    }
}

/// RPC / ABCI entry-point for anchoring an entangled edge in 5D-QEH.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MsgAnchorEdge {
    pub request_id: u64,
    pub chain_epoch: u64,
    pub parents: Vec<VertexId>,
    pub parent_coherence: f64,
    pub lamport: u64,
    pub contribution_score: f64,
    pub ann_similarity: f32,
    pub qrng_entropy_bits: u16,
    pub pqc_binding: PqcBinding,
    pub icosuple: Icosuple,
}

impl MsgAnchorEdge {
    pub fn weight_input(&self) -> TemporalWeightInput {
        TemporalWeightInput::new(
            self.lamport,
            self.parent_coherence,
            self.qrng_entropy_bits,
            self.contribution_score,
            self.ann_similarity,
            self.icosuple.entanglement_coefficient as f64,
        )
    }

    /// Deterministic preimage that is signed by PQC bindings when anchoring.
    pub fn signing_preimage(&self) -> [u8; 32] {
        anchor_signing_preimage(self)
    }
}

/// Response returned after anchoring an edge via RPCNet.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MsgAnchorEdgeResponse {
    pub receipt: VertexReceipt,
    pub storage: ModuleStorageLayout,
}

fn anchor_signing_preimage(msg: &MsgAnchorEdge) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(&msg.request_id.to_le_bytes());
    hasher.update(&msg.chain_epoch.to_le_bytes());
    hasher.update(&msg.lamport.to_le_bytes());
    hasher.update(&msg.parent_coherence.to_le_bytes());
    hasher.update(&msg.contribution_score.to_le_bytes());
    hasher.update(&msg.ann_similarity.to_le_bytes());
    hasher.update(&msg.qrng_entropy_bits.to_le_bytes());
    hasher.update(msg.pqc_binding.key_id.as_bytes());
    match &msg.pqc_binding.scheme {
        PqcScheme::Kyber => {
            hasher.update(b"kyber");
        }
        PqcScheme::Dilithium => {
            hasher.update(b"dilithium");
        }
        PqcScheme::Falcon => {
            hasher.update(b"falcon");
        }
        PqcScheme::Hybrid(label) => {
            hasher.update(label.as_bytes());
        }
    };
    for parent in &msg.parents {
        hasher.update(parent.as_bytes());
    }
    hasher.update(msg.icosuple.label.as_bytes());
    hasher.update(&msg.icosuple.payload_bytes.to_le_bytes());
    for layer in &msg.icosuple.pqc_layers {
        match &layer.scheme {
            PqcScheme::Kyber => {
                hasher.update(b"layer/kyber");
            }
            PqcScheme::Dilithium => {
                hasher.update(b"layer/dilithium");
            }
            PqcScheme::Falcon => {
                hasher.update(b"layer/falcon");
            }
            PqcScheme::Hybrid(label) => {
                hasher.update(b"layer/hybrid/");
                hasher.update(label.as_bytes());
            }
        };
        hasher.update(layer.metadata_tag.as_bytes());
        hasher.update(&layer.epoch.to_le_bytes());
    }
    for value in &msg.icosuple.vector_signature {
        hasher.update(&value.to_le_bytes());
    }
    msg.icosuple.quantum_coordinates.hash_into(&mut hasher);
    hasher.update(&msg.icosuple.entanglement_coefficient.to_le_bytes());
    msg.icosuple.crystalline_voxel.hash_into(&mut hasher);
    msg.icosuple.laser_link.hash_into(&mut hasher);
    let mut out = [0u8; 32];
    out.copy_from_slice(hasher.finalize().as_bytes());
    out
}

/// Errors returned by the chain module.
#[derive(thiserror::Error, Debug, PartialEq)]
pub enum ModuleError {
    #[error(transparent)]
    Hypergraph(#[from] HypergraphError),
    #[error("parent coherence must be within [0,1], got {0}")]
    InvalidParentCoherence(f64),
    #[error(transparent)]
    Pqc(#[from] PqcRuntimeError),
}

/// Chain-ready facade that wraps the deterministic hypergraph state machine.
pub struct HypergraphModule {
    state: HypergraphState,
    weight_model: TemporalWeightModel,
    storage: ModuleStorageLayout,
    pqc_runtime: Option<Arc<dyn PqcRuntime>>,
}

impl HypergraphModule {
    pub fn new(config: QehConfig, weight_model: TemporalWeightModel) -> Self {
        Self {
            state: HypergraphState::new(config),
            weight_model,
            storage: ModuleStorageLayout::default(),
            pqc_runtime: None,
        }
    }

    pub fn config(&self) -> &QehConfig {
        self.state.config()
    }

    pub fn weight_model(&self) -> &TemporalWeightModel {
        &self.weight_model
    }

    pub fn storage_layout(&self) -> &ModuleStorageLayout {
        &self.storage
    }

    /// Attach a PQC runtime (native or WASM) so anchor edges can be signed.
    pub fn with_pqc_runtime(mut self, runtime: Arc<dyn PqcRuntime>) -> Self {
        self.pqc_runtime = Some(runtime);
        self
    }

    /// Replace the PQC runtime at runtime (e.g., when hot-swapping engines).
    pub fn set_pqc_runtime(&mut self, runtime: Arc<dyn PqcRuntime>) {
        self.pqc_runtime = Some(runtime);
    }

    /// Detach the PQC runtime (tests, dev harnesses, or PQC-disabled builds).
    pub fn clear_pqc_runtime(&mut self) {
        self.pqc_runtime = None;
    }

    pub fn state(&self) -> &HypergraphState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut HypergraphState {
        &mut self.state
    }

    pub fn apply_anchor_edge(&mut self, msg: MsgAnchorEdge) -> Result<VertexReceipt, ModuleError> {
        if !(0.0..=1.0).contains(&msg.parent_coherence) {
            return Err(ModuleError::InvalidParentCoherence(msg.parent_coherence));
        }
        let pqc_signature = if let Some(runtime) = self.pqc_runtime.as_ref() {
            let digest = msg.signing_preimage();
            let signature = msg
                .pqc_binding
                .request_signature(runtime.as_ref(), &digest)
                .map_err(ModuleError::Pqc)?;
            let epoch_ms = msg.chain_epoch.saturating_mul(1_000);
            let _ = msg
                .pqc_binding
                .rotate_if_needed(runtime.as_ref(), epoch_ms)
                .map_err(ModuleError::Pqc)?;
            Some(signature)
        } else {
            None
        };
        let weight_input = msg.weight_input();
        let MsgAnchorEdge {
            icosuple, parents, ..
        } = msg;
        let receipt = self.state.insert(
            icosuple,
            parents,
            &self.weight_model,
            weight_input,
            pqc_signature,
        )?;
        self.storage.register(&receipt);
        Ok(receipt)
    }
}

/// Intent used by the developer simulation harness.
#[cfg(feature = "sim")]
#[derive(Clone, Debug)]
pub struct SimulationIntent {
    pub label: String,
    pub parents: Vec<VertexId>,
    pub payload_bytes: usize,
    pub lamport: u64,
    pub contribution_score: f64,
    pub ann_similarity: f32,
    pub qrng_entropy_bits: u16,
}

#[cfg(feature = "sim")]
impl SimulationIntent {
    pub fn entangle(
        label: impl Into<String>,
        parents: Vec<VertexId>,
        payload_bytes: usize,
        lamport: u64,
        contribution_score: f64,
        ann_similarity: f32,
        qrng_entropy_bits: u16,
    ) -> Self {
        Self {
            label: label.into(),
            parents,
            payload_bytes,
            lamport,
            contribution_score,
            ann_similarity,
            qrng_entropy_bits,
        }
    }

    pub fn into_anchor_edge(self, config: &QehConfig, chain_epoch: u64) -> MsgAnchorEdge {
        let parent_count = self.parents.len();
        let parent_coherence = if parent_count == 0 {
            0.1
        } else {
            (parent_count as f64 / config.max_parent_links as f64).min(1.0)
        };
        let request_id = chain_epoch
            .wrapping_mul(1_000_000)
            .wrapping_add(self.lamport)
            .wrapping_add(parent_count as u64);
        let icosuple =
            Icosuple::synthesize(config, self.label, self.payload_bytes, self.ann_similarity);
        MsgAnchorEdge {
            request_id,
            chain_epoch,
            parents: self.parents,
            parent_coherence,
            lamport: self.lamport,
            contribution_score: self.contribution_score,
            ann_similarity: self.ann_similarity,
            qrng_entropy_bits: self.qrng_entropy_bits,
            pqc_binding: PqcBinding::simulated("sim-harness"),
            icosuple,
        }
    }
}

/// Pulsed laser telemetry emitted by the simulator.
#[cfg(feature = "sim")]
pub type LaserPath = PulsedLaserLink;

/// Output of a simulator epoch.
#[cfg(feature = "sim")]
#[derive(Clone, Debug)]
pub struct SimulationReport {
    pub epoch_index: u64,
    pub accepted_vertices: usize,
    pub rejected_vertices: usize,
    pub avg_temporal_weight: f64,
    pub coherence_index: f64,
    pub crystalline_archives: usize,
    pub hot_set_vertices: usize,
    pub laser_paths: Vec<LaserPath>,
    pub storage_layout: ModuleStorageLayout,
}

/// Deterministic simulator used by demos/tests.
#[cfg(feature = "sim")]
pub struct FiveDqehSim {
    config: QehConfig,
    weight_model: TemporalWeightModel,
    rng: QrngEntropyRng,
    epoch: u64,
}

#[cfg(feature = "sim")]
impl FiveDqehSim {
    pub fn with_seed(seed: u64, config: QehConfig, weight_model: TemporalWeightModel) -> Self {
        Self {
            rng: QrngEntropyRng::with_seed(seed),
            config,
            weight_model,
            epoch: 0,
        }
    }

    pub fn drive_epoch<I>(&mut self, module: &mut HypergraphModule, intents: I) -> SimulationReport
    where
        I: IntoIterator<Item = SimulationIntent>,
    {
        debug_assert_eq!(self.weight_model, *module.weight_model());

        let mut accepted = 0usize;
        let mut rejected = 0usize;
        let mut weight_sum = 0.0;
        let mut coherence_sum = 0.0;
        let mut crystalline = 0usize;
        let mut hot = 0usize;

        for intent in intents {
            let msg = intent.into_anchor_edge(module.config(), self.epoch);
            match module.apply_anchor_edge(msg) {
                Ok(receipt) => {
                    accepted += 1;
                    weight_sum += receipt.tw_score;
                    coherence_sum += receipt.ann_similarity as f64;
                    match receipt.storage {
                        StorageTarget::Crystalline => crystalline += 1,
                        StorageTarget::Hot => hot += 1,
                    }
                }
                Err(_) => {
                    rejected += 1;
                }
            }
        }

        let avg_temporal_weight = if accepted > 0 {
            weight_sum / accepted as f64
        } else {
            0.0
        };
        let coherence_index = if accepted > 0 {
            (coherence_sum / accepted as f64).min(1.0)
        } else {
            0.0
        };

        let laser_paths = self.emit_laser_paths();
        let report = SimulationReport {
            epoch_index: self.epoch,
            accepted_vertices: accepted,
            rejected_vertices: rejected,
            avg_temporal_weight,
            coherence_index,
            crystalline_archives: crystalline,
            hot_set_vertices: hot,
            laser_paths,
            storage_layout: module.storage_layout().clone(),
        };
        self.epoch += 1;
        report
    }

    fn emit_laser_paths(&mut self) -> Vec<LaserPath> {
        let mut paths = Vec::with_capacity(self.config.laser_channels as usize);
        for channel in 0..self.config.laser_channels {
            let throughput_gbps = self.rng.gen_range_f64(1_000.0..=1_000_000.0);
            let latency_ps = self.rng.gen_range_f64(0.5..=10.0);
            let qkd_active = self.rng.gen_bool(0.85);
            paths.push(PulsedLaserLink::new(
                channel,
                throughput_gbps,
                latency_ps,
                qkd_active,
            ));
        }
        paths
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporal_weight_respects_entropy() {
        let model = TemporalWeightModel::default();
        let low = TemporalWeightInput::new(10, 0.2, 64, 0.1, 0.5, 0.4);
        let high = TemporalWeightInput::new(10, 0.2, 512, 0.1, 0.5, 0.4);
        assert!(model.score(&high) > model.score(&low));
    }

    #[test]
    fn hypergraph_enforces_parent_limit() {
        let mut config = QehConfig::default();
        config.max_parent_links = 2;
        let mut state = HypergraphState::new(config.clone());
        let model = TemporalWeightModel::default();
        let icosuple = Icosuple::synthesize(&config, "demo", 1_024, 0.9);
        let mut parent_rng = HostEntropySource::new();
        let parents = vec![
            VertexId::random(&mut parent_rng),
            VertexId::random(&mut parent_rng),
            VertexId::random(&mut parent_rng),
        ];
        let input = TemporalWeightInput::new(5, 1.0, 256, 0.2, 0.9, 0.85);
        let err = state
            .insert(icosuple, parents, &model, input, None)
            .expect_err("too many parents");
        assert!(matches!(err, HypergraphError::TooManyParents { .. }));
    }

    struct MockRuntime;

    impl PqcRuntime for MockRuntime {
        fn pqc_handshake(
            &self,
            _binding: &PqcBinding,
            _request: &PqcHandshakeRequest,
        ) -> Result<PqcHandshakeReceipt, PqcRuntimeError> {
            Err(PqcRuntimeError::Disabled)
        }

        fn pqc_sign(
            &self,
            _binding: &PqcBinding,
            payload: &[u8],
        ) -> Result<PqcSignature, PqcRuntimeError> {
            Ok(PqcSignature {
                key_id: "mock".into(),
                bytes: payload.to_vec(),
            })
        }

        fn pqc_rotate(
            &self,
            _binding: &PqcBinding,
            _now_ms: u64,
        ) -> Result<PqcRotationOutcome, PqcRuntimeError> {
            Ok(PqcRotationOutcome {
                rotated: true,
                old_key: Some("old".into()),
                new_key: Some("new".into()),
            })
        }
    }

    #[test]
    fn module_attaches_pqc_signature_when_runtime_available() {
        let config = QehConfig::default();
        let model = TemporalWeightModel::default();
        let runtime = Arc::new(MockRuntime);
        let mut module = HypergraphModule::new(config.clone(), model).with_pqc_runtime(runtime);
        let icosuple = Icosuple::synthesize(&config, "edge", 2_048, 0.91);
        let msg = MsgAnchorEdge {
            request_id: 99,
            chain_epoch: 42,
            parents: vec![],
            parent_coherence: 0.25,
            lamport: 7,
            contribution_score: 0.5,
            ann_similarity: 0.91,
            qrng_entropy_bits: 512,
            pqc_binding: PqcBinding::new("did:autheo:validator/mock", PqcScheme::Dilithium),
            icosuple,
        };
        let digest = msg.signing_preimage();
        let receipt = module.apply_anchor_edge(msg).expect("anchor edge");
        let signature = receipt.pqc_signature.expect("signature attached");
        assert_eq!(signature.key_id, "mock");
        assert_eq!(signature.bytes, digest);
    }

    #[cfg(feature = "sim")]
    #[test]
    fn simulator_reports_activity() {
        let config = QehConfig::default();
        let model = TemporalWeightModel::default();
        let mut module = HypergraphModule::new(config.clone(), model);
        let mut sim = FiveDqehSim::with_seed(42, config.clone(), model);
        let mut parent_rng = QrngEntropyRng::with_seed(7);
        let intents = vec![
            SimulationIntent::entangle("genesis", vec![], 2_048, 1, 0.4, 0.82, 256),
            SimulationIntent::entangle(
                "edge-channel",
                vec![VertexId::random(&mut parent_rng)],
                3_000,
                2,
                0.6,
                0.74,
                384,
            ),
        ];
        let report = sim.drive_epoch(&mut module, intents);
        assert!(report.accepted_vertices >= 1);
        assert_eq!(report.laser_paths.len(), config.laser_channels as usize);
        assert_eq!(
            report.storage_layout.total_vertices(),
            report.accepted_vertices
        );
    }
}
