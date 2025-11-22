//! Chronosync QS-DAG consensus primitives for Autheo PQCNet.
//!
//! The crate captures the essentials of the Chronosync design: time-weighted validator
//! profiles, QRNG-driven verification pools, QS-DAG witnesses, and a lightweight simulator
//! that surfaces throughput plus fairness metrics for future dedicated repos or Cosmos SDK
//! modules. Everything is parameterized so downstream tooling can tune shard counts,
//! subpool sizes, and TPS ceilings without rewriting the core heuristics.

#[cfg(feature = "sim")]
use rand::{
    distributions::{Distribution, WeightedIndex},
    rngs::StdRng,
    seq::SliceRandom,
    Rng, SeedableRng,
};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
#[cfg(feature = "sim")]
use std::collections::HashSet;
use std::hash::{Hash, Hasher};

/// Chronosync DAG nodes reference at most 10 parents.
pub const MAX_PARENT_REFERENCES: usize = 10;

/// Tunable parameters for Chronosync simulations and demos.
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

impl ChronosyncConfig {
    #[cfg(feature = "sim")]
    fn validate(&self) {
        assert!(self.shards > 0, "Chronosync requires at least one shard");
        assert!(self.layers > 0, "Chronosync requires at least one layer");
        assert!(
            self.verification_pools > 0,
            "At least one verification pool is required"
        );
        assert!(self.subpool_size > 0, "Subpool size must be non-zero");
        assert!(self.max_parents > 0 && self.max_parents <= MAX_PARENT_REFERENCES);
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

/// Chronosync simulator used by demos, tests, and notebooks.
#[cfg(feature = "sim")]
pub struct ChronosyncSim<R> {
    config: ChronosyncConfig,
    rng: R,
    epoch: u64,
}

#[cfg(feature = "sim")]
impl ChronosyncSim<StdRng> {
    /// Deterministic constructor used by examples/tests.
    pub fn with_seed(seed: u64, config: ChronosyncConfig) -> Self {
        Self::new(config, StdRng::seed_from_u64(seed))
    }
}

#[cfg(feature = "sim")]
impl<R: Rng> ChronosyncSim<R> {
    pub fn new(config: ChronosyncConfig, rng: R) -> Self {
        config.validate();
        Self {
            config,
            rng,
            epoch: 0,
        }
    }

    pub fn config(&self) -> &ChronosyncConfig {
        &self.config
    }

    /// Run a single epoch, returning pool elections, shard telemetry, and a DAG witness.
    pub fn drive_epoch(
        &mut self,
        nodes: &[ChronosyncNodeProfile],
        transactions: u64,
    ) -> EpochReport {
        assert!(
            !nodes.is_empty(),
            "ChronosyncSim requires at least one validator profile"
        );

        let epoch_index = self.epoch;
        let capped_tps = transactions.min(self.config.global_tps) as f64;
        let pools = self.elect_pools(nodes);
        let fairness_gini = gini(
            &nodes
                .iter()
                .map(|n| n.temporal_weight())
                .collect::<Vec<_>>(),
        );
        let dag_witness = self.build_dag(nodes, transactions);
        let shard_utilization = self.sample_shards(capped_tps, &pools);
        let rejection_rate = (fairness_gini * 0.05).min(0.05);
        let rejected_transactions = (capped_tps * rejection_rate).round() as u64;

        self.epoch += 1;

        EpochReport {
            epoch_index,
            aggregated_tps: capped_tps,
            fairness_gini,
            pools,
            shard_utilization,
            dag_witness,
            rejected_transactions,
        }
    }

    fn elect_pools(&mut self, nodes: &[ChronosyncNodeProfile]) -> Vec<VerificationPoolSnapshot> {
        let weights: Vec<f64> = nodes
            .iter()
            .map(|node| node.temporal_weight().max(1e-6))
            .collect();
        let dist = WeightedIndex::new(&weights).expect("non-empty weights");
        let mut pools = Vec::with_capacity(self.config.verification_pools);

        for pool_id in 0..self.config.verification_pools {
            let mut selections = Vec::with_capacity(self.config.subpool_size);
            let mut used_indexes = HashSet::new();
            for _ in 0..self.config.subpool_size {
                let mut attempts = 0usize;
                let idx = loop {
                    let candidate = dist.sample(&mut self.rng);
                    attempts += 1;
                    if used_indexes.insert(candidate) || attempts > nodes.len() * 2 {
                        break candidate;
                    }
                };
                let profile = &nodes[idx];
                selections.push(NodeSelection {
                    node_id: profile.node_id.clone(),
                    time_weight: weights[idx],
                    shard_affinity: profile.shard_affinity(self.config.shards),
                    longevity_hours: profile.longevity_hours,
                    proof_of_burn_tokens: profile.proof_of_burn_tokens,
                    zkp_validations: profile.zkp_validations,
                });
            }
            pools.push(VerificationPoolSnapshot {
                pool_id: pool_id as u16,
                selections,
            });
        }

        pools
    }

    fn build_dag(&mut self, nodes: &[ChronosyncNodeProfile], transactions: u64) -> DagWitness {
        let per_layer_txs = (transactions.min(self.config.global_tps) / self.config.layers as u64)
            .max(1)
            .min(self.config.max_layer_tps);
        let mut history: Vec<String> = Vec::new();
        let mut dag_nodes = Vec::with_capacity(self.config.layers as usize);

        for layer in 0..self.config.layers {
            let leader = nodes
                .choose(&mut self.rng)
                .map(|profile| profile.node_id.clone())
                .expect("nodes is non-empty");

            let parents = if history.is_empty() {
                Vec::new()
            } else {
                let parent_count = usize::min(self.config.max_parents, history.len());
                history
                    .choose_multiple(&mut self.rng, parent_count)
                    .cloned()
                    .collect()
            };

            let leader_profile = nodes
                .iter()
                .find(|profile| profile.node_id == leader)
                .expect("leader must exist");

            let node_id = format!("epoch{}-layer{}-node{}", self.epoch, layer, history.len());
            history.push(node_id.clone());

            dag_nodes.push(DagNode {
                node_id,
                parents,
                shard_affinity: leader_profile.shard_affinity(self.config.shards),
                leader,
                payload_bytes: 400,
                transactions_carried: per_layer_txs,
            });
        }

        DagWitness { nodes: dag_nodes }
    }

    fn sample_shards(
        &mut self,
        total_tps: f64,
        pools: &[VerificationPoolSnapshot],
    ) -> Vec<ShardLoad> {
        let per_shard = total_tps / self.config.shards as f64;
        let mut iter = pools.iter().flat_map(|pool| pool.selections.iter());
        let mut shard_loads = Vec::with_capacity(self.config.shards as usize);

        for shard_id in 0..self.config.shards {
            let jitter = self.rng.gen_range(0.85..1.15);
            let leader = iter
                .find(|selection| selection.shard_affinity == shard_id)
                .map(|selection| selection.node_id.clone());
            shard_loads.push(ShardLoad {
                shard_id,
                throughput_tps: per_shard * jitter,
                tier_path: DEFAULT_TIER_PATH,
                elected_leader: leader,
            });
        }

        shard_loads
    }
}

#[cfg(feature = "sim")]
fn gini(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len() as f64;
    let sum: f64 = sorted.iter().sum();
    if sum == 0.0 {
        return 0.0;
    }
    let mut cumulative = 0.0;
    for (i, value) in sorted.iter().enumerate() {
        cumulative += (i as f64 + 1.0) * *value;
    }
    let g = (2.0 * cumulative) / (n * sum) - (n + 1.0) / n;
    g.max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn gini_handles_zero_values() {
        let values = vec![0.0, 0.0, 0.0];
        assert_eq!(gini(&values), 0.0);
    }
}
