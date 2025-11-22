//! Chronosync QS-DAG consensus primitives for Autheo PQCNet.
//!
//! The crate captures the production entry points from the Chronosync design: time-weighted validator
//! profiles, QRNG-driven verification pools, QS-DAG witnesses, and the keeper that hydrates 5D-QEH.
//! Everything is parameterized so downstream tooling can tune shard counts, subpool sizes, and TPS
//! ceilings without rewriting the core heuristics (simulators now live in higher-level repos).

pub mod keeper;
pub use keeper::{ChronosyncKeeper, ChronosyncKeeperError, ChronosyncKeeperReport};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

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
            layers: 3,
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
    pub suspicious_events: u32,
}

impl TemporalWeightInput {
    /// Evaluate the TW formula with logarithmic longevity rewards, capped PoB/ZKP contributions,
    /// and multiplicative suspicion decay.
    pub fn compute(&self) -> f64 {
        let longevity_term = ((self.longevity_hours as f64) / 24.0 + 1.0).ln();
        let pob_term = 0.2 * self.proof_of_burn_tokens.min(1.0);
        let zkp_term = 0.1 * ((self.zkp_validations as f64 / 1_000.0).min(1.0));
        let suspicion_penalty = (self.suspicious_events as f64 * 0.05).min(0.5);
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
            suspicious_events: 0,
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
    pub suspicious_events: u32,
}

impl ChronosyncNodeProfile {
    pub fn new(node_id: impl Into<String>) -> Self {
        Self {
            node_id: node_id.into(),
            longevity_hours: 0,
            proof_of_burn_tokens: 0.0,
            zkp_validations: 0,
            suspicious_events: 0,
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

    pub fn with_suspicion_events(mut self, events: u32) -> Self {
        self.suspicious_events = events;
        self
    }

    pub fn temporal_weight(&self) -> f64 {
        TemporalWeightInput {
            longevity_hours: self.longevity_hours,
            proof_of_burn_tokens: self.proof_of_burn_tokens,
            zkp_validations: self.zkp_validations,
            suspicious_events: self.suspicious_events,
        }
        .compute()
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
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeSelection {
    pub node_id: String,
    pub time_weight: f64,
    pub shard_affinity: u16,
    pub longevity_hours: u64,
    pub proof_of_burn_tokens: f64,
    pub zkp_validations: u64,
}

/// Snapshot of a verification pool with its sub-pool participants.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VerificationPoolSnapshot {
    pub pool_id: u16,
    pub selections: Vec<NodeSelection>,
}

/// Per-shard throughput telemetry emitted by the simulator.
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


#[cfg(test)]
mod tests {
    use super::*;
    use autheo_pqcnet_5dqeh::{
        HypergraphModule, HostEntropySource, Icosuple, MsgAnchorEdge, PqcBinding, PqcLayer,
        PqcScheme, QehConfig, TemporalWeightModel, VertexId, ICOSUPLE_BYTES,
    };
    use autheo_pqcnet_tuplechain::{
        ProofScheme, TupleChainConfig, TupleChainKeeper, TuplePayload, TupleReceipt,
    };
    use pqcnet_networking::AnchorEdgeEndpoint;

    #[test]
    fn temporal_weight_matches_formula() {
        let input = TemporalWeightInput {
            longevity_hours: 24 * 30,
            proof_of_burn_tokens: 0.7,
            zkp_validations: 700,
            suspicious_events: 1,
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
        let mut keeper =
            ChronosyncKeeper::new(chrono_config, HypergraphModule::new(qeh_config, tw_model));
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
            icosuple: Icosuple::synthesize("rpcnet", 1_024, 8, 0.9),
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
        let mut keeper =
            ChronosyncKeeper::new(chrono_config.clone(), HypergraphModule::new(qeh_config.clone(), tw_model));

        let mut entropy = HostEntropySource::new();
        let parents: Vec<VertexId> = (0..2).map(|_| VertexId::random(&mut entropy)).collect();
        let parent_coherence =
            (parents.len() as f64 / qeh_config.max_parent_links as f64).min(1.0);

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
        let mut keeper =
            TupleChainKeeper::new(TupleChainConfig::default()).allow_creator("did:autheo:l1/kernel");
        let payload = TuplePayload::builder("did:autheo:alice", "owns")
            .object_text("autheo-passport")
            .proof(ProofScheme::Zkp, b"proof", "zkp")
            .expiry(1_700_000_000_000)
            .build();
        keeper
            .store_tuple("did:autheo:l1/kernel", payload, 1_700_000_000_000)
            .expect("tuple receipt")
    }
}
