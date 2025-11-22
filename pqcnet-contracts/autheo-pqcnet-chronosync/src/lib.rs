//! Chronosync QS-DAG consensus primitives for Autheo PQCNet.
//!
//! The crate captures the production entry points from the Chronosync design: time-weighted validator
//! profiles, QRNG-driven verification pools, QS-DAG witnesses, and the keeper that hydrates 5D-QEH.
//! Everything is parameterized so downstream tooling can tune shard counts, subpool sizes, and TPS
//! ceilings while operating directly on production telemetry.

pub mod keeper;
use autheo_pqcnet_qrng::QrngEntropyFrame;
pub use keeper::{ChronosyncKeeper, ChronosyncKeeperError, ChronosyncKeeperReport};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use thiserror::Error;

/// Chronosync DAG nodes reference at most 10 parents.
pub const MAX_PARENT_REFERENCES: usize = 10;

/// Tunable parameters for Chronosync deployments and diagnostics.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChronosyncConfig {
    pub shards: u16,
    pub layers: u8,
    pub verification_pools: usize,
    pub subpool_size: usize,
    pub max_parents: usize,
    pub max_layer_tps: u64,
    pub global_tps: u64,
    pub qrng_entropy_bits: u16,
}

impl Default for ChronosyncConfig {
    fn default() -> Self {
        Self {
            shards: 1_000,
            layers: 21,
            verification_pools: 10,
            subpool_size: 5,
            max_parents: MAX_PARENT_REFERENCES,
            max_layer_tps: 1_000_000_000,
            global_tps: 50_000_000_000,
            qrng_entropy_bits: 256,
        }
    }
}

/// Inputs used to compute the Temporal Weight (TW) score described in the Chronosync primer.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TemporalWeightInput {
    pub longevity_hours: u64,
    pub proof_of_burn_tokens: f64,
    pub zkp_validations: u64,
    pub suspicion_hours: u64,
}

impl TemporalWeightInput {
    /// Evaluate the TW formula with logarithmic longevity rewards, capped PoB/ZKP contributions,
    /// and multiplicative suspicion decay.
    pub fn compute(&self) -> f64 {
        let longevity_term = ((self.longevity_hours as f64) / 24.0 + 1.0).ln();
        let pob_term = 0.2 * self.proof_of_burn_tokens.min(1.0);
        let zkp_term = 0.1 * ((self.zkp_validations as f64 / 1_000.0).min(1.0));
        let suspicion_penalty = (self.suspicion_hours as f64 * 0.05).min(0.5);
        let decay = 1.0 - suspicion_penalty;
        let score = (longevity_term + pob_term + zkp_term) * decay;
        score.min(1.0).max(0.0)
    }
}

impl Default for TemporalWeightInput {
    fn default() -> Self {
        Self {
            longevity_hours: 0,
            proof_of_burn_tokens: 0.0,
            zkp_validations: 0,
            suspicion_hours: 0,
        }
    }
}

/// Validator metadata tracked by Chronosync.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChronosyncNodeProfile {
    pub node_id: String,
    pub longevity_hours: u64,
    pub proof_of_burn_tokens: f64,
    pub zkp_validations: u64,
    pub suspicion_hours: u64,
}

impl ChronosyncNodeProfile {
    pub fn new(node_id: impl Into<String>) -> Self {
        Self {
            node_id: node_id.into(),
            longevity_hours: 0,
            proof_of_burn_tokens: 0.0,
            zkp_validations: 0,
            suspicion_hours: 0,
        }
    }

    pub fn with_longevity_hours(mut self, hours: u64) -> Self {
        self.longevity_hours = hours;
        self
    }

    pub fn with_proof_of_burn(mut self, tokens: f64) -> Self {
        self.proof_of_burn_tokens = tokens;
        self
    }

    pub fn with_zkp_validations(mut self, count: u64) -> Self {
        self.zkp_validations = count;
        self
    }

    pub fn with_suspicion_hours(mut self, hours: u64) -> Self {
        self.suspicion_hours = hours;
        self
    }

    pub fn temporal_weight(&self) -> f64 {
        TemporalWeightInput {
            longevity_hours: self.longevity_hours,
            proof_of_burn_tokens: self.proof_of_burn_tokens,
            zkp_validations: self.zkp_validations,
            suspicion_hours: self.suspicion_hours,
        }
        .compute()
    }

    /// Reputation decays by 0.05 per hour of suspicious behavior, capped at zero.
    pub fn reputation_score(&self) -> f64 {
        (1.0 - (self.suspicion_hours as f64 * 0.05)).clamp(0.0, 1.0)
    }

    pub fn shard_affinity(&self, shards: u16) -> u16 {
        let mut hasher = DefaultHasher::new();
        self.node_id.hash(&mut hasher);
        let shard = (hasher.finish() % shards as u64) as u16;
        shard
    }
}

/// Compressed representation of Tuplechain -> Icosuple -> QS-DAG tiers.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum TierDesignation {
    Tuplechain,
    IcosupleCore,
    QsDag,
}

/// Default path applied to shard utilization reporting.
pub const DEFAULT_TIER_PATH: [TierDesignation; 3] = [
    TierDesignation::Tuplechain,
    TierDesignation::IcosupleCore,
    TierDesignation::QsDag,
];

/// Node assignment inside a verification sub-pool.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NodeSelection {
    pub node_id: String,
    pub time_weight: f64,
    pub reputation: f64,
    pub shard_affinity: u16,
    pub longevity_hours: u64,
    pub proof_of_burn_tokens: f64,
    pub zkp_validations: u64,
}

/// Snapshot of a verification pool with its sub-pool participants.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct VerificationPoolSnapshot {
    pub pool_id: u16,
    pub selections: Vec<NodeSelection>,
}

/// Per-shard throughput telemetry emitted by the Chronosync runtime.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShardLoad {
    pub shard_id: u16,
    pub throughput_tps: f64,
    pub tier_path: [TierDesignation; 3],
    pub elected_leader: Option<String>,
}

/// DAG witness propagated to tuplechain / icosuple layers.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DagNode {
    pub node_id: String,
    pub parents: Vec<String>,
    pub shard_affinity: u16,
    pub leader: String,
    pub payload_bytes: usize,
    pub transactions_carried: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DagWitness {
    pub nodes: Vec<DagNode>,
}

/// Aggregate metrics for a Chronosync epoch.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EpochReport {
    pub epoch_index: u64,
    pub aggregated_tps: f64,
    pub fairness_gini: f64,
    pub pools: Vec<VerificationPoolSnapshot>,
    pub shard_utilization: Vec<ShardLoad>,
    pub dag_witness: DagWitness,
    pub rejected_transactions: u64,
}

/// Errors raised while electing Chronosync verification pools from QRNG data.
#[derive(Debug, Error)]
pub enum PoolElectionError {
    #[error(
        "verification pools require positive counts (pools={pools}, subpool_size={subpool_size})"
    )]
    InvalidTopology { pools: usize, subpool_size: usize },
    #[error("insufficient validator profiles: required {required}, available {available}")]
    InsufficientProfiles { required: usize, available: usize },
    #[error("insufficient QRNG frames: required {required}, available: {available}")]
    InsufficientQrngFrames { required: usize, available: usize },
}

/// Use QRNG entropy frames and Temporal Weight profiles to elect verification pools.
///
/// The selection process hashes real QRNG entropy (photonic, vacuum, chip) into per-pool seeds,
/// weights candidates by their TW × reputation score, and deterministically chooses
/// `config.subpool_size` members for each pool. The function errors if insufficient telemetry or
/// profiles are supplied, guaranteeing parity with the Chronosync production spec.
pub fn elect_verification_pools(
    config: &ChronosyncConfig,
    profiles: &[ChronosyncNodeProfile],
    qrng_frames: &[QrngEntropyFrame],
    epoch_index: u64,
) -> Result<Vec<VerificationPoolSnapshot>, PoolElectionError> {
    if config.verification_pools == 0 || config.subpool_size == 0 {
        return Err(PoolElectionError::InvalidTopology {
            pools: config.verification_pools,
            subpool_size: config.subpool_size,
        });
    }

    let required_profiles = config.verification_pools * config.subpool_size;
    if profiles.len() < required_profiles {
        return Err(PoolElectionError::InsufficientProfiles {
            required: required_profiles,
            available: profiles.len(),
        });
    }

    let required_frames = 3.min(config.verification_pools.max(1));
    if qrng_frames.len() < required_frames {
        return Err(PoolElectionError::InsufficientQrngFrames {
            required: required_frames,
            available: qrng_frames.len(),
        });
    }

    let seeds = derive_pool_seeds(config.verification_pools, qrng_frames, epoch_index);
    let mut snapshots = Vec::with_capacity(config.verification_pools);
    for (idx, seed) in seeds.into_iter().enumerate() {
        snapshots.push(select_pool(idx as u16, seed, config, profiles));
    }
    Ok(snapshots)
}

fn derive_pool_seeds(
    pool_count: usize,
    frames: &[QrngEntropyFrame],
    epoch_index: u64,
) -> Vec<[u8; 32]> {
    let shares = frames.len().min(5).max(1);
    (0..pool_count)
        .map(|pool_idx| {
            let mut hasher = Sha3_256::new();
            hasher.update(b"chronosync/pool_seed");
            hasher.update(&epoch_index.to_le_bytes());
            hasher.update(&(pool_idx as u64).to_le_bytes());
            for share_idx in 0..shares {
                let frame_index = (pool_idx + share_idx * pool_count) % frames.len();
                let frame = &frames[frame_index];
                mix_frame_into_hasher(&mut hasher, frame);
            }
            let digest: [u8; 32] = hasher.finalize().into();
            digest
        })
        .collect()
}

fn mix_frame_into_hasher(hasher: &mut Sha3_256, frame: &QrngEntropyFrame) {
    hasher.update(&frame.checksum);
    hasher.update(&frame.timestamp_ps.to_le_bytes());
    hasher.update(&frame.epoch.to_le_bytes());
    hasher.update(&frame.sequence.to_le_bytes());
    hasher.update(frame.request.label.as_bytes());
    hasher.update(&frame.request.bits.to_le_bytes());
    hasher.update(frame.request.icosuple_reference.as_bytes());
    hasher.update(&frame.envelope.qrng_entropy_bits.to_le_bytes());
    hasher.update(frame.envelope.icosuple_reference.as_bytes());
    hasher.update(&frame.entropy);
    for source in &frame.sources {
        hasher.update(source.source.as_bytes());
        hasher.update(&source.bits.to_le_bytes());
        hasher.update(&source.shot_count.to_le_bytes());
        hasher.update(&source.bias_ppm.to_le_bytes());
        hasher.update(&source.drift_ppm.to_le_bytes());
        hasher.update(&source.raw_entropy);
    }
}

fn select_pool(
    pool_id: u16,
    seed: [u8; 32],
    config: &ChronosyncConfig,
    profiles: &[ChronosyncNodeProfile],
) -> VerificationPoolSnapshot {
    let mut candidates: Vec<(f64, NodeSelection)> = profiles
        .iter()
        .map(|profile| {
            let time_weight = profile.temporal_weight();
            let reputation = profile.reputation_score();
            let priority = pool_priority(&seed, &profile.node_id, time_weight, reputation);
            let node = NodeSelection {
                node_id: profile.node_id.clone(),
                time_weight,
                reputation,
                shard_affinity: profile.shard_affinity(config.shards),
                longevity_hours: profile.longevity_hours,
                proof_of_burn_tokens: profile.proof_of_burn_tokens,
                zkp_validations: profile.zkp_validations,
            };
            (priority, node)
        })
        .collect();

    candidates.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let selections = candidates
        .into_iter()
        .take(config.subpool_size)
        .map(|(_, node)| node)
        .collect();

    VerificationPoolSnapshot {
        pool_id,
        selections,
    }
}

fn pool_priority(seed: &[u8; 32], node_id: &str, time_weight: f64, reputation: f64) -> f64 {
    let mut hasher = Sha3_256::new();
    hasher.update(seed);
    hasher.update(node_id.as_bytes());
    let digest = hasher.finalize();
    let mut seed_bytes = [0u8; 8];
    seed_bytes.copy_from_slice(&digest[..8]);
    let raw = u64::from_le_bytes(seed_bytes) as f64 / u64::MAX as f64;
    let weight = (time_weight * reputation).max(1e-9);
    raw / weight
}

#[cfg(test)]
mod tests {
    use super::*;
    use autheo_pqcnet_5dqeh::{
        CrystallineVoxel, HostEntropySource, HypergraphModule, Icosuple, MsgAnchorEdge, PqcBinding,
        PqcLayer, PqcScheme, PulsedLaserLink, QehConfig, QuantumCoordinates, TemporalWeightModel,
        VertexId, ICOSUPLE_BYTES,
    };
    use autheo_pqcnet_qrng::{EntropyRequest, QrngMixer};
    use autheo_pqcnet_tuplechain::{
        ProofScheme, TupleChainConfig, TupleChainKeeper, TuplePayload, TupleReceipt,
    };
    use pqcnet_networking::AnchorEdgeEndpoint;

    #[test]
    fn pool_election_requires_sufficient_profiles() {
        let mut config = ChronosyncConfig::default();
        config.verification_pools = 2;
        config.subpool_size = 3;
        let profiles = sample_profiles(4);
        let frames = qrng_frames(3);
        let err = elect_verification_pools(&config, &profiles, &frames, 11)
            .expect_err("must reject insufficient profiles");
        match err {
            PoolElectionError::InsufficientProfiles {
                required,
                available,
            } => {
                assert_eq!(required, 6);
                assert_eq!(available, 4);
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn pool_election_is_deterministic_with_shared_entropy() {
        let mut config = ChronosyncConfig::default();
        config.verification_pools = 3;
        config.subpool_size = 4;
        let profiles = sample_profiles(32);
        let frames = qrng_frames(5);
        let first = elect_verification_pools(&config, &profiles, &frames, 22).unwrap();
        let second = elect_verification_pools(&config, &profiles, &frames, 22).unwrap();
        assert_eq!(
            first, second,
            "deterministic seeds must yield identical selections"
        );
        for snapshot in &first {
            assert_eq!(snapshot.selections.len(), config.subpool_size);
            assert!(snapshot
                .selections
                .iter()
                .all(|sel| sel.time_weight <= 1.0 && sel.reputation <= 1.0));
        }
    }

    #[test]
    fn temporal_weight_matches_formula() {
        let input = TemporalWeightInput {
            longevity_hours: 24 * 30,
            proof_of_burn_tokens: 0.7,
            zkp_validations: 700,
            suspicion_hours: 1,
        };
        let score = input.compute();
        let expected = ((input.longevity_hours as f64) / 24.0 + 1.0).ln()
            + 0.2 * input.proof_of_burn_tokens
            + 0.1 * (input.zkp_validations as f64 / 1_000.0);
        let expected = expected * (1.0 - 0.05);
        assert!((score - expected.min(1.0)).abs() < 1e-6);
    }

    fn sample_report(nodes: Vec<DagNode>) -> EpochReport {
        EpochReport {
            epoch_index: 7,
            aggregated_tps: 1_000.0,
            fairness_gini: 0.2,
            pools: Vec::new(),
            shard_utilization: Vec::new(),
            dag_witness: DagWitness { nodes },
            rejected_transactions: 0,
        }
    }

    #[test]
    fn keeper_streams_dag_witness_into_hypergraph() {
        let chrono_config = ChronosyncConfig::default();
        let qeh_config = QehConfig::default();
        let tw_model = TemporalWeightModel::default();
        let module = HypergraphModule::new(qeh_config, tw_model);
        let mut keeper = ChronosyncKeeper::new(chrono_config, module);

        let nodes = vec![
            DagNode {
                node_id: "node-0".into(),
                parents: Vec::new(),
                shard_affinity: 0,
                leader: "did:autheo:alpha".into(),
                payload_bytes: 2_048,
                transactions_carried: 500,
            },
            DagNode {
                node_id: "node-1".into(),
                parents: vec!["node-0".into()],
                shard_affinity: 1,
                leader: "did:autheo:beta".into(),
                payload_bytes: 3_072,
                transactions_carried: 700,
            },
        ];
        let report = sample_report(nodes);
        let outcome = keeper.ingest_epoch_report(&report).expect("ingest epoch");
        assert_eq!(outcome.applied_vertices.len(), 2);
        assert!(outcome.missing_parents.is_empty());
        assert!(outcome.storage_layout.total_vertices() >= 2);
        assert!(outcome.dag_head.is_some());
    }

    #[test]
    fn keeper_handles_anchor_edge_requests_via_rpcnet_trait() {
        let chrono_config = ChronosyncConfig::default();
        let qeh_config = QehConfig::default();
        let tw_model = TemporalWeightModel::default();
        let mut keeper = ChronosyncKeeper::new(
            chrono_config,
            HypergraphModule::new(qeh_config.clone(), tw_model),
        );
        let msg = MsgAnchorEdge {
            request_id: 1,
            chain_epoch: 1,
            parents: Vec::new(),
            parent_coherence: 0.1,
            lamport: 1,
            contribution_score: 0.5,
            ann_similarity: 0.9,
            qrng_entropy_bits: 256,
            pqc_binding: PqcBinding::new("did:autheo:alpha", PqcScheme::Dilithium),
            icosuple: Icosuple::synthesize(&qeh_config, "rpcnet", 1_024, 0.9),
        };
        let response = AnchorEdgeEndpoint::submit_anchor_edge(&mut keeper, msg).expect("anchor");
        assert_eq!(response.receipt.parents, 0);
        assert_eq!(response.storage.total_vertices(), 1);
    }

    #[test]
    fn tuplechain_receipts_anchor_edges_through_chronosync() {
        let receipt = production_tuple_receipt();
        let chrono_config = ChronosyncConfig::default();
        let qeh_config = QehConfig::default();
        let tw_model = TemporalWeightModel::default();
        let mut keeper = ChronosyncKeeper::new(
            chrono_config.clone(),
            HypergraphModule::new(qeh_config.clone(), tw_model),
        );

        let mut entropy = HostEntropySource::new();
        let parents: Vec<VertexId> = (0..2).map(|_| VertexId::random(&mut entropy)).collect();
        let parent_coherence = (parents.len() as f64 / qeh_config.max_parent_links as f64).min(1.0);

        let vector_signature = vec![0.91_f32; qeh_config.vector_dimensions];
        let pqc_layers = receipt
            .tier_path
            .iter()
            .enumerate()
            .map(|(idx, tier)| PqcLayer {
                scheme: if idx == 0 {
                    PqcScheme::Kyber
                } else {
                    PqcScheme::Dilithium
                },
                metadata_tag: format!("tier-{}::{tier:?}", idx),
                epoch: receipt.version as u64,
            })
            .collect();
        let icosuple = Icosuple {
            label: format!("tuple/{}", receipt.tuple_id),
            payload_bytes: ICOSUPLE_BYTES,
            pqc_layers,
            vector_signature,
            quantum_coordinates: QuantumCoordinates::default(),
            entanglement_coefficient: 0.94,
            crystalline_voxel: CrystallineVoxel::default(),
            laser_link: PulsedLaserLink::default(),
        };

        let msg = MsgAnchorEdge {
            request_id: 42,
            chain_epoch: receipt.expiry / 1_000_000,
            parents: parents.clone(),
            parent_coherence,
            lamport: receipt.version as u64,
            contribution_score: 0.72,
            ann_similarity: 0.94,
            qrng_entropy_bits: chrono_config.qrng_entropy_bits,
            pqc_binding: PqcBinding::new(receipt.creator.clone(), PqcScheme::Dilithium),
            icosuple,
        };

        let response =
            AnchorEdgeEndpoint::submit_anchor_edge(&mut keeper, msg).expect("anchor tuple receipt");
        assert_eq!(response.receipt.parents, parents.len());
        assert!(
            response.storage.total_vertices() >= 1,
            "storage layout must register anchored vertex"
        );
        assert!(
            keeper
                .module()
                .state()
                .get(&response.receipt.vertex_id)
                .is_some(),
            "vertex should be queryable from hypergraph state"
        );
    }

    fn production_tuple_receipt() -> TupleReceipt {
        let mut keeper = TupleChainKeeper::new(TupleChainConfig::default())
            .allow_creator("did:autheo:l1/kernel");
        let payload = TuplePayload::builder("did:autheo:alice", "owns")
            .object_text("autheo-passport")
            .proof(ProofScheme::Zkp, b"proof", "zkp")
            .expiry(1_700_000_000_000)
            .build();
        keeper
            .store_tuple("did:autheo:l1/kernel", payload, 1_700_000_000_000)
            .expect("tuple receipt")
    }

    fn sample_profiles(count: usize) -> Vec<ChronosyncNodeProfile> {
        (0..count)
            .map(|idx| {
                ChronosyncNodeProfile::new(format!("did:autheo:validator:{idx}"))
                    .with_longevity_hours(24 * (idx as u64 + 1))
                    .with_proof_of_burn(((idx + 1) as f64 / 50.0).min(1.0))
                    .with_zkp_validations((idx as u64 + 1) * 100)
                    .with_suspicion_hours((idx as u64 % 4) as u64)
            })
            .collect()
    }

    fn qrng_frames(count: usize) -> Vec<QrngEntropyFrame> {
        let mut mixer = QrngMixer::new(0x5a5a_0420);
        (0..count)
            .map(|idx| {
                let request = EntropyRequest::for_icosuple(
                    "chronosync",
                    512 + (idx as u16 * 16),
                    format!("ico-{idx}"),
                );
                mixer.generate_frame(7, idx as u64, request)
            })
            .collect()
    }
}
