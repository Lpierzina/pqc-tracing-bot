 //! 5D-QEH (Five-Dimensional Qubit-Enhanced Hypergraph) primitives for Autheo PQCNet.
 //!
 //! The crate models the core structures that make 5D-QEH distinct from classical DAGs:
 //! 4096-byte icosuples, temporal-weighted entanglement, and pulsed-laser propagation
 //! channels that bind TupleChain, QS-DAG and AI agents together. It also exposes a
 //! lightweight simulator so notebooks, demos, and sentry prototypes can reason about
 //! throughput, crystalline offloading, and QKD-protected laser meshes without bringing
 //! the entire Autheo-One stack into scope.
 
 use blake3::Hasher;
 use rand::{rngs::StdRng, Rng, SeedableRng};
 use serde::{Deserialize, Serialize};
 use std::collections::BTreeMap;
 
 /// Canonical byte size for 5D-QEH icosuples.
 pub const ICOSUPLE_BYTES: usize = 4096;
 
 /// Describes how many parents a vertex is allowed to entangle with.
 pub const MAX_SIM_PARENT_LINKS: usize = 100;
 
 /// High-level configuration shared by the hypergraph state machine and simulator.
 #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
 pub struct QehConfig {
     pub max_parent_links: usize,
     pub ann_similarity_threshold: f32,
     pub crystalline_offload_after_ms: u64,
     pub crystalline_payload_threshold: usize,
     pub laser_channels: u16,
     pub vector_dimensions: usize,
 }
 
 impl Default for QehConfig {
     fn default() -> Self {
         Self {
             max_parent_links: MAX_SIM_PARENT_LINKS,
             ann_similarity_threshold: 0.78,
             crystalline_offload_after_ms: 2_592_000_000, // 30 days in milliseconds
             crystalline_payload_threshold: 3_584,
             laser_channels: 16,
             vector_dimensions: 8,
         }
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
 }
 
 impl TemporalWeightModel {
     pub const fn new(
         lamport_gain: f64,
         coherence_gain: f64,
         entropy_gain: f64,
         contribution_gain: f64,
         ann_gain: f64,
     ) -> Self {
         Self {
             lamport_gain,
             coherence_gain,
             entropy_gain,
             contribution_gain,
             ann_gain,
         }
     }
 
     /// Evaluate the temporal weight given the observed entanglement metrics.
     pub fn score(&self, input: &TemporalWeightInput) -> f64 {
         let lamport_term = ((input.lamport as f64 / 1_000.0) + 1.0).ln() * self.lamport_gain;
         let coherence_term = input
             .parent_coherence
             .clamp(0.0, 1.0)
             * self.coherence_gain;
         let entropy_term =
             ((input.qrng_entropy_bits as f64) / 512.0).min(1.0) * self.entropy_gain;
         let contribution_term = input.contribution_score * self.contribution_gain;
         let ann_term = input.ann_similarity as f64 * self.ann_gain;
         (lamport_term + coherence_term + entropy_term + contribution_term + ann_term)
             .clamp(0.0, 10.0)
     }
 }
 
 impl Default for TemporalWeightModel {
     fn default() -> Self {
         Self::new(0.65, 2.1, 1.3, 0.9, 1.4)
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
 }
 
 impl TemporalWeightInput {
     pub fn new(
         lamport: u64,
         parent_coherence: f64,
         qrng_entropy_bits: u16,
         contribution_score: f64,
         ann_similarity: f32,
     ) -> Self {
         Self {
             lamport,
             parent_coherence,
             qrng_entropy_bits,
             contribution_score,
             ann_similarity,
         }
     }
 }
 
 /// Compact identifier for hypergraph vertices.
 #[derive(
     Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
 )]
 pub struct VertexId(pub [u8; 32]);
 
 impl VertexId {
     pub fn random<R: Rng>(rng: &mut R) -> Self {
         let mut bytes = [0u8; 32];
         rng.fill(&mut bytes);
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
 }
 
 impl Icosuple {
     /// Synthesizes an icosuple for demos/simulations.
     pub fn synthesize(
         label: impl Into<String>,
         payload_bytes: usize,
         vector_dimensions: usize,
         similarity_hint: f32,
     ) -> Self {
         let label = label.into();
         let dims = vector_dimensions.max(1);
         let normalized = similarity_hint.clamp(0.0, 1.0);
         let mut vector_signature = Vec::with_capacity(dims);
         for i in 0..dims {
             let phase = ((i as f32) * 0.37).sin().abs();
             let blended = ((phase * 0.5) + normalized).min(1.0);
             vector_signature.push(blended);
         }
 
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
         let mut bytes = [0u8; 32];
         bytes.copy_from_slice(hasher.finalize().as_bytes());
         VertexId(bytes)
     }
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
 }
 
 /// Storage placement for an icosuple.
 #[derive(Clone, Debug, PartialEq, Eq)]
 pub enum StorageTarget {
     Hot,
     Crystalline,
 }
 
 /// Receipt returned after inserting a vertex.
 #[derive(Clone, Debug)]
 pub struct VertexReceipt {
     pub vertex_id: VertexId,
     pub tw_score: f64,
     pub storage: StorageTarget,
     pub ann_similarity: f32,
     pub parents: usize,
 }
 
 /// Materialized vertex information.
 #[derive(Clone, Debug)]
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
 
     pub fn insert(
         &mut self,
         icosuple: Icosuple,
         parents: Vec<VertexId>,
         model: &TemporalWeightModel,
         tw_input: TemporalWeightInput,
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
 
     fn should_archive(
         &self,
         icosuple: &Icosuple,
         tw_input: &TemporalWeightInput,
     ) -> bool {
         tw_input.lamport >= self.config.crystalline_offload_after_ms
             || icosuple.payload_bytes >= self.config.crystalline_payload_threshold
             || tw_input.ann_similarity < self.config.ann_similarity_threshold
     }
 }
 
 /// Intent used by the simulation harness.
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
 }
 
 /// Pulsed laser telemetry emitted by the simulator.
 #[derive(Clone, Debug)]
 pub struct LaserPath {
     pub channel_id: u16,
     pub throughput_gbps: f64,
     pub latency_ps: f64,
     pub qkd_active: bool,
 }
 
 /// Output of a simulator epoch.
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
 }
 
 /// Deterministic simulator used by demos/tests.
 pub struct FiveDqehSim {
     config: QehConfig,
     weight_model: TemporalWeightModel,
     rng: StdRng,
     epoch: u64,
 }
 
 impl FiveDqehSim {
     pub fn with_seed(seed: u64, config: QehConfig, weight_model: TemporalWeightModel) -> Self {
         Self {
             rng: StdRng::seed_from_u64(seed),
             config,
             weight_model,
             epoch: 0,
         }
     }
 
     pub fn drive_epoch<I>(
         &mut self,
         state: &mut HypergraphState,
         intents: I,
     ) -> SimulationReport
     where
         I: IntoIterator<Item = SimulationIntent>,
     {
         debug_assert_eq!(self.config.vector_dimensions, state.config().vector_dimensions);
         debug_assert_eq!(self.config.max_parent_links, state.config().max_parent_links);
 
         let mut accepted = 0usize;
         let mut rejected = 0usize;
         let mut weight_sum = 0.0;
         let mut coherence_sum = 0.0;
         let mut crystalline = 0usize;
         let mut hot = 0usize;
 
         for intent in intents {
             let parents = intent.parents;
             let parent_coherence = if parents.is_empty() {
                 0.1
             } else {
                 (parents.len() as f64 / self.config.max_parent_links as f64).min(1.0)
             };
             let icosuple = Icosuple::synthesize(
                 intent.label,
                 intent.payload_bytes,
                 self.config.vector_dimensions,
                 intent.ann_similarity,
             );
             let weight_input = TemporalWeightInput::new(
                 intent.lamport,
                 parent_coherence,
                 intent.qrng_entropy_bits,
                 intent.contribution_score,
                 intent.ann_similarity,
             );
             match state.insert(icosuple, parents, &self.weight_model, weight_input) {
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
         };
         self.epoch += 1;
         report
     }
 
     fn emit_laser_paths(&mut self) -> Vec<LaserPath> {
         let mut paths = Vec::with_capacity(self.config.laser_channels as usize);
         for channel in 0..self.config.laser_channels {
             let throughput_gbps = self.rng.gen_range(1_000.0..=1_000_000.0);
             let latency_ps = self.rng.gen_range(0.5..=10.0);
             let qkd_active = self.rng.gen_bool(0.85);
             paths.push(LaserPath {
                 channel_id: channel,
                 throughput_gbps,
                 latency_ps,
                 qkd_active,
             });
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
         let low = TemporalWeightInput::new(10, 0.2, 64, 0.1, 0.5);
         let high = TemporalWeightInput::new(10, 0.2, 512, 0.1, 0.5);
         assert!(model.score(&high) > model.score(&low));
     }
 
     #[test]
     fn hypergraph_enforces_parent_limit() {
         let mut config = QehConfig::default();
         config.max_parent_links = 2;
         let mut state = HypergraphState::new(config.clone());
         let model = TemporalWeightModel::default();
         let icosuple = Icosuple::synthesize("demo", 1024, config.vector_dimensions, 0.9);
         let parents = vec![
             VertexId::random(&mut StdRng::seed_from_u64(1)),
             VertexId::random(&mut StdRng::seed_from_u64(2)),
             VertexId::random(&mut StdRng::seed_from_u64(3)),
         ];
         let input = TemporalWeightInput::new(5, 1.0, 256, 0.2, 0.9);
        let err = state
            .insert(icosuple, parents, &model, input)
            .expect_err("too many parents");
        assert!(matches!(err, HypergraphError::TooManyParents { .. }));
     }
 
     #[test]
     fn simulator_reports_activity() {
         let config = QehConfig::default();
         let mut state = HypergraphState::new(config.clone());
         let model = TemporalWeightModel::default();
         let mut sim = FiveDqehSim::with_seed(42, config.clone(), model);
         let intents = vec![
             SimulationIntent::entangle("genesis", vec![], 2_048, 1, 0.4, 0.82, 256),
             SimulationIntent::entangle(
                 "edge-channel",
                 vec![VertexId::random(&mut StdRng::seed_from_u64(7))],
                 3_000,
                 2,
                 0.6,
                 0.74,
                 384,
             ),
         ];
         let report = sim.drive_epoch(&mut state, intents);
         assert!(report.accepted_vertices >= 1);
         assert_eq!(
             report.laser_paths.len(),
             config.laser_channels as usize
         );
     }
 }
