//! Icosuple n-tier network primitives for the Autheo-One DeOS stack.
//!
//! The crate keeps the architectural promises from the Autheo primer:
//! - Every icosuple extends the 3072-byte tuplechain payload to 4096 bytes with layer-specific metadata
//!   and PQC signatures.
//! - Tier specialisations (compute, storage, identity, metaverse, etc.) can be simulated so architects
//!   can reason about >20 layers today and infinite tiers tomorrow.
//! - Chronosync / QS-DAG hooks are modelled as telemetry so future dedicated repos can lift this crate
//!   without rewriting the primitives.

use rand::{
    distributions::{Distribution, WeightedIndex},
    rngs::StdRng,
    Rng, SeedableRng,
};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Base tuple bytes anchored in tuplechain.
pub const BASE_TUPLE_BYTES: usize = 3072;
/// Each icosuple inflates tuples to 4,096 bytes.
pub const ICOSUPLE_BYTES: usize = 4096;
const HASH_FIELD_BYTES: usize = 1024;
const METADATA_BYTES: usize = 512;
const PQC_SIGNATURE_BYTES: usize = 512;

/// Intent flowing out of Tuplechain into the Icosuple fabric.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TupleIntent {
    pub subject: String,
    pub domain: String,
    pub payload_bytes: usize,
    pub ttl_ms: u64,
    pub estimated_tps: f64,
    pub layer_hint: Option<u8>,
}

impl TupleIntent {
    pub fn new(
        subject: impl Into<String>,
        domain: impl Into<String>,
        payload_bytes: usize,
    ) -> Self {
        Self {
            subject: subject.into(),
            domain: domain.into(),
            payload_bytes,
            ttl_ms: 86_400_000,
            estimated_tps: 1.0,
            layer_hint: None,
        }
    }

    pub fn identity(
        subject: impl Into<String>,
        credential: impl Into<String>,
        ttl_ms: u64,
    ) -> Self {
        Self {
            ttl_ms,
            ..Self::new(subject, credential, 2_048)
        }
    }

    pub fn with_estimated_tps(mut self, tps: f64) -> Self {
        self.estimated_tps = tps.max(0.1);
        self
    }

    pub fn with_layer_hint(mut self, tier: u8) -> Self {
        self.layer_hint = Some(tier);
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HashField {
    pub label: String,
    pub bytes: usize,
}

impl HashField {
    pub fn new(label: &'static str) -> Self {
        Self {
            label: label.to_string(),
            bytes: HASH_FIELD_BYTES,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Icosuple {
    pub id: String,
    pub subject: String,
    pub domain: String,
    pub input_hash: HashField,
    pub prev_hash: HashField,
    pub current_hash: HashField,
    pub metadata: LayerMetadata,
    pub pqc_signature: PqcEnvelope,
}

impl Icosuple {
    pub fn total_bytes(&self) -> usize {
        self.input_hash.bytes
            + self.prev_hash.bytes
            + self.current_hash.bytes
            + self.metadata.bytes()
            + self.pqc_signature.bytes()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayerMetadata {
    pub tier_assignments: Vec<LayerAssignment>,
    pub vector_embedding_dims: u16,
    pub entanglement_coefficient: f32,
    pub ttl_ms: u64,
}

impl LayerMetadata {
    pub fn bytes(&self) -> usize {
        METADATA_BYTES
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayerAssignment {
    pub tier_index: u8,
    pub specialization: TierSpecialization,
    pub shard_id: u16,
    pub throughput_tps: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PqcEnvelope {
    pub kyber_level: u8,
    pub dilithium_level: u8,
    pub qrng_entropy_bits: u16,
}

impl PqcEnvelope {
    pub fn bytes(&self) -> usize {
        PQC_SIGNATURE_BYTES
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum TierSpecialization {
    DeosKernel,
    TuplechainAnchor,
    ComputeHash,
    StorageHash,
    MessagingHash,
    IdentityHash,
    CredentialHash,
    AiHash,
    MlHash,
    DataHash,
    FinanceHash,
    HealthHash,
    IotHash,
    MetaverseHash,
    GovHash,
    EnergyHash,
    PrivacyHash,
    BountyHash,
    QuantumHash,
    InteropHash,
    ExtensionHash,
    Dynamic(u8),
}

impl TierSpecialization {
    pub fn from_tier_number(tier: u8) -> Self {
        match tier {
            0 => Self::DeosKernel,
            1 => Self::TuplechainAnchor,
            2 => Self::ComputeHash,
            3 => Self::StorageHash,
            4 => Self::MessagingHash,
            5 => Self::IdentityHash,
            6 => Self::CredentialHash,
            7 => Self::AiHash,
            8 => Self::MlHash,
            9 => Self::DataHash,
            10 => Self::FinanceHash,
            11 => Self::HealthHash,
            12 => Self::IotHash,
            13 => Self::MetaverseHash,
            14 => Self::GovHash,
            15 => Self::EnergyHash,
            16 => Self::PrivacyHash,
            17 => Self::BountyHash,
            18 => Self::QuantumHash,
            19 => Self::InteropHash,
            20 => Self::ExtensionHash,
            other => Self::Dynamic(other),
        }
    }

    pub fn label(&self) -> Cow<'static, str> {
        use TierSpecialization::*;
        match self {
            DeosKernel => Cow::Borrowed("Layer 0 · DeOS kernel"),
            TuplechainAnchor => Cow::Borrowed("Layer 1 · tuplechain anchor"),
            ComputeHash => Cow::Borrowed("Layer 2 · compute hash"),
            StorageHash => Cow::Borrowed("Layer 3 · storage hash"),
            MessagingHash => Cow::Borrowed("Layer 4 · messaging hash"),
            IdentityHash => Cow::Borrowed("Layer 5 · identity hash"),
            CredentialHash => Cow::Borrowed("Layer 6 · credential hash"),
            AiHash => Cow::Borrowed("Layer 7 · AI hash"),
            MlHash => Cow::Borrowed("Layer 8 · ML hash"),
            DataHash => Cow::Borrowed("Layer 9 · data hash"),
            FinanceHash => Cow::Borrowed("Layer 10 · finance hash"),
            HealthHash => Cow::Borrowed("Layer 11 · health hash"),
            IotHash => Cow::Borrowed("Layer 12 · IoT hash"),
            MetaverseHash => Cow::Borrowed("Layer 13 · metaverse hash"),
            GovHash => Cow::Borrowed("Layer 14 · gov hash"),
            EnergyHash => Cow::Borrowed("Layer 15 · energy hash"),
            PrivacyHash => Cow::Borrowed("Layer 16 · privacy hash"),
            BountyHash => Cow::Borrowed("Layer 17 · bounty hash"),
            QuantumHash => Cow::Borrowed("Layer 18 · quantum hash"),
            InteropHash => Cow::Borrowed("Layer 19 · interop hash"),
            ExtensionHash => Cow::Borrowed("Layer 20 · extension hash"),
            Dynamic(n) => Cow::Owned(format!("Layer {} · dynamic tier", n)),
        }
    }

    pub fn description(&self) -> &'static str {
        use TierSpecialization::*;
        match self {
            DeosKernel => {
                "DeOS kernel orchestrates quantum VMs, storage crystals, and RPCNet overlays."
            }
            TuplechainAnchor => {
                "Tuplechain summarizes 3072-byte tuples with ZK-rollups at 10M TPS."
            }
            ComputeHash => "GPU/TPU heavy compute tier delivering 100M TPS for DePIN workloads.",
            StorageHash => {
                "Crystalline storage tier providing 360TB/mm³ density for archival tuples."
            }
            MessagingHash => "Sub-picosecond messaging mesh for entangled RPC frames.",
            IdentityHash => "ZKP-protected SSI anchors for AutheoID and DID pipelines.",
            CredentialHash => "MPC-secured verifiable credentials with selective disclosure.",
            AiHash => "Vector embeddings + qubit simulations targeting 1B TPS per layer.",
            MlHash => "Training tier for agents with 1B parameter windows and FHE gradients.",
            DataHash => "FHE analytics tier enabling encrypted data streams and ANN lookups.",
            FinanceHash => "Deterministic settlement tier for DeFi rails and CBDC rollups.",
            HealthHash => "FHIR-compliant EMR tier for Aurkei / Autheo clinical workloads.",
            IotHash => "Lightweight sensor/IoT tier bridging wearables + DePIN inputs.",
            MetaverseHash => "Metaverse + digital-twin tier with entangled asset states.",
            GovHash => "Temporal-weighted governance and DAO orchestration tier.",
            EnergyHash => "Proof-of-Burn + energy market tier optimising microgrids.",
            PrivacyHash => "Stealth addresses, zero-leakage analytics, and mixnet bridges.",
            BountyHash => "Reward orchestration tier for verifiers/bounty hunters.",
            QuantumHash => "QKD-secured state tier storing entangled witnesses.",
            InteropHash => "IBC/RPCNet bridging tier enabling cross-chain overlays.",
            ExtensionHash => "Tier spawning new shards / overlays for N+1 layers.",
            Dynamic(_) => "Late-bound tier added via governance for domain specialists.",
        }
    }

    pub fn target_tps(&self) -> u64 {
        use TierSpecialization::*;
        match self {
            DeosKernel => 5_000_000,
            TuplechainAnchor => 10_000_000,
            ComputeHash => 100_000_000,
            StorageHash => 80_000_000,
            MessagingHash => 120_000_000,
            IdentityHash => 60_000_000,
            CredentialHash => 60_000_000,
            AiHash => 1_000_000_000,
            MlHash => 900_000_000,
            DataHash => 700_000_000,
            FinanceHash => 400_000_000,
            HealthHash => 350_000_000,
            IotHash => 500_000_000,
            MetaverseHash => 800_000_000,
            GovHash => 200_000_000,
            EnergyHash => 250_000_000,
            PrivacyHash => 300_000_000,
            BountyHash => 150_000_000,
            QuantumHash => 100_000_000,
            InteropHash => 220_000_000,
            ExtensionHash => 1_000_000_000,
            Dynamic(_) => 1_200_000_000,
        }
    }

    fn throughput_weight(&self) -> f64 {
        use TierSpecialization::*;
        match self {
            DeosKernel | TuplechainAnchor => 0.4,
            ComputeHash => 0.9,
            StorageHash => 0.8,
            MessagingHash => 0.7,
            IdentityHash | CredentialHash => 0.6,
            AiHash | MlHash => 1.2,
            DataHash => 1.0,
            FinanceHash => 0.75,
            HealthHash => 0.65,
            IotHash => 0.8,
            MetaverseHash => 1.05,
            GovHash => 0.5,
            EnergyHash => 0.55,
            PrivacyHash => 0.7,
            BountyHash => 0.45,
            QuantumHash => 0.4,
            InteropHash => 0.7,
            ExtensionHash => 1.1,
            Dynamic(_) => 1.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TierParameters {
    pub index: u8,
    pub specialization: TierSpecialization,
    pub shards: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TierReport {
    pub tier_index: u8,
    pub specialization: TierSpecialization,
    pub shards: u16,
    pub throughput_tps: f64,
    pub saturation: f64,
    pub description: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NetworkTelemetry {
    pub epoch: u64,
    pub aggregated_tps: f64,
    pub tiers: Vec<TierReport>,
    pub icosuples: Vec<Icosuple>,
    pub qs_dag_edges: usize,
    pub dynamic_extensions: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IcosupleNetworkConfig {
    pub tier_count: u8,
    pub shards_per_tier: u16,
    pub max_shards: u16,
    pub max_layer_tps: u64,
    pub global_tps: u64,
    pub qrng_entropy_bits: u16,
    pub vector_embedding_dims: u16,
    pub assignments_per_icosuple: usize,
}

impl Default for IcosupleNetworkConfig {
    fn default() -> Self {
        Self {
            tier_count: 20,
            shards_per_tier: 1_000,
            max_shards: 32_768,
            max_layer_tps: 1_000_000_000,
            global_tps: 50_000_000_000,
            qrng_entropy_bits: 512,
            vector_embedding_dims: 4_096,
            assignments_per_icosuple: 5,
        }
    }
}

impl IcosupleNetworkConfig {
    pub fn with_tier_count(mut self, tier_count: u8) -> Self {
        self.tier_count = tier_count.max(1);
        self
    }

    pub fn with_assignments_per_icosuple(mut self, count: usize) -> Self {
        self.assignments_per_icosuple = count.max(1).min(16);
        self
    }
}

pub struct IcosupleNetworkSim {
    config: IcosupleNetworkConfig,
    rng: StdRng,
    epoch: u64,
}

impl IcosupleNetworkSim {
    pub fn with_seed(seed: u64, config: IcosupleNetworkConfig) -> Self {
        let rng = StdRng::seed_from_u64(seed);
        Self {
            config,
            rng,
            epoch: 0,
        }
    }

    pub fn config(&self) -> &IcosupleNetworkConfig {
        &self.config
    }

    pub fn propagate_batch(&mut self, intents: &[TupleIntent]) -> NetworkTelemetry {
        assert!(
            !intents.is_empty(),
            "at least one tuple intent is required to build icosuples"
        );
        let catalog = self.build_tier_catalog();
        let total_weight: f64 = intents.iter().map(|intent| intent.estimated_tps).sum();
        let aggregated_tps = total_weight.min(self.config.global_tps as f64);
        let tier_reports = self.generate_tier_reports(aggregated_tps, &catalog);
        let assignments = std::cmp::min(self.config.assignments_per_icosuple, tier_reports.len());
        let icosuples = intents
            .iter()
            .enumerate()
            .map(|(idx, intent)| {
                self.materialize_icosuple(idx as u64, intent, &tier_reports, assignments)
            })
            .collect::<Vec<_>>();

        let dynamic_extensions = tier_reports
            .iter()
            .filter(|report| {
                matches!(
                    report.specialization,
                    TierSpecialization::ExtensionHash | TierSpecialization::Dynamic(_)
                )
            })
            .count();
        let qs_dag_edges = tier_reports.len() * assignments;

        let telemetry = NetworkTelemetry {
            epoch: self.epoch,
            aggregated_tps,
            tiers: tier_reports,
            icosuples,
            qs_dag_edges,
            dynamic_extensions,
        };
        self.epoch += 1;
        telemetry
    }

    fn build_tier_catalog(&self) -> Vec<TierParameters> {
        let mut tiers = Vec::new();
        let total_layers = self.config.tier_count as u16 + 2;
        for tier_index in 0..total_layers {
            let specialization = TierSpecialization::from_tier_number(tier_index as u8);
            let shards = match specialization {
                TierSpecialization::DeosKernel => 64,
                TierSpecialization::TuplechainAnchor => self.config.shards_per_tier.min(4_096),
                _ => {
                    let incremental = self.config.shards_per_tier.saturating_add(tier_index * 13);
                    incremental.min(self.config.max_shards)
                }
            };
            tiers.push(TierParameters {
                index: tier_index as u8,
                specialization,
                shards,
            });
        }
        tiers
    }

    fn generate_tier_reports(
        &mut self,
        aggregated_tps: f64,
        catalog: &[TierParameters],
    ) -> Vec<TierReport> {
        catalog
            .iter()
            .map(|params| {
                let target = (params.specialization.target_tps() as f64)
                    .min(self.config.max_layer_tps as f64);
                let jitter = self.rng.gen_range(0.9..1.1);
                let throughput =
                    (aggregated_tps * params.specialization.throughput_weight() * jitter
                        / catalog.len() as f64)
                        .min(target);
                let saturation = (throughput / target).min(1.2);
                TierReport {
                    tier_index: params.index,
                    specialization: params.specialization,
                    shards: params.shards,
                    throughput_tps: throughput,
                    saturation,
                    description: params.specialization.description().to_string(),
                }
            })
            .collect()
    }

    fn materialize_icosuple(
        &mut self,
        ordinal: u64,
        intent: &TupleIntent,
        tiers: &[TierReport],
        assignments: usize,
    ) -> Icosuple {
        let weights: Vec<f64> = tiers.iter().map(|report| 0.1 + report.saturation).collect();
        let dist = WeightedIndex::new(&weights).expect("tier weights should be non-zero");

        let mut tier_assignments = Vec::with_capacity(assignments);
        for _ in 0..assignments {
            let idx = dist.sample(&mut self.rng);
            let report = &tiers[idx];
            let shard = hash_to_shard(&intent.subject, report.shards);
            tier_assignments.push(LayerAssignment {
                tier_index: report.tier_index,
                specialization: report.specialization,
                shard_id: shard,
                throughput_tps: report.throughput_tps,
            });
        }

        let entanglement = (tier_assignments.len() as f32 * 0.05) + 0.5;
        let ttl_ms = intent.ttl_ms;

        Icosuple {
            id: format!("ico-{}-{}", self.epoch, ordinal),
            subject: intent.subject.clone(),
            domain: intent.domain.clone(),
            input_hash: HashField::new("input_hash@1024"),
            prev_hash: HashField::new("prev_hash@1024"),
            current_hash: HashField::new("current_hash@1024"),
            metadata: LayerMetadata {
                tier_assignments,
                vector_embedding_dims: self.config.vector_embedding_dims,
                entanglement_coefficient: entanglement,
                ttl_ms,
            },
            pqc_signature: PqcEnvelope {
                kyber_level: 5,
                dilithium_level: 5,
                qrng_entropy_bits: self.config.qrng_entropy_bits,
            },
        }
    }
}

fn hash_to_shard(subject: &str, shards: u16) -> u16 {
    if shards == 0 {
        return 0;
    }
    let mut hasher = DefaultHasher::new();
    subject.hash(&mut hasher);
    (hasher.finish() % shards as u64) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specialization_mapping_covers_first_20_layers() {
        for tier in 0..=20 {
            let spec = TierSpecialization::from_tier_number(tier);
            assert_ne!(spec.target_tps(), 0);
        }
    }

    #[test]
    fn icosuple_respects_size_budget() {
        let config = IcosupleNetworkConfig::default();
        let mut sim = IcosupleNetworkSim::with_seed(7, config);
        let intents =
            vec![
                TupleIntent::identity("did:autheo:alice", "autheoid-passport", 86_400_000)
                    .with_estimated_tps(5_000_000.0),
            ];
        let telemetry = sim.propagate_batch(&intents);
        assert_eq!(telemetry.icosuples.len(), 1);
        let ico = &telemetry.icosuples[0];
        assert_eq!(ico.total_bytes(), ICOSUPLE_BYTES);
    }

    #[test]
    fn telemetry_tracks_dynamic_extensions() {
        let config = IcosupleNetworkConfig::default().with_tier_count(22);
        let mut sim = IcosupleNetworkSim::with_seed(42, config);
        let intents = vec![
            TupleIntent::new("did:autheo:bob", "depin-meter", 1_024)
                .with_estimated_tps(10_000_000.0),
            TupleIntent::new("did:autheo:carol", "metaverse-passport", 4_096)
                .with_estimated_tps(2_000_000.0),
        ];
        let telemetry = sim.propagate_batch(&intents);
        assert!(telemetry.dynamic_extensions >= 1);
    }
}
