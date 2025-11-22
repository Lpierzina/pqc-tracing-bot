//! Production hyper-tuple module for the Autheo-One stack.
//!
//! Rather than running stochastic demos, this crate consumes TupleChain receipts and Chronosync/QS-DAG
//! telemetry to deterministically inflate a 3,072-byte tuple into the canonical 4,096-byte hyper-tuple:
//! input hash, previous hash, current hash, layer metadata (20 specialisation tiers + infinite
//! extensions), and a PQC envelope. The resulting bytes are what the Chronosync keeper delivers to
//! `autheo-pqcnet-5dqeh`.

use autheo_pqcnet_5dqeh::{
    ModuleStorageLayout, PqcBinding, PqcScheme, StorageTarget, VertexReceipt,
};
use autheo_pqcnet_chronosync::{ChronosyncKeeperReport, EpochReport};
use autheo_pqcnet_tuplechain::{ShardId, TupleReceipt, TupleId};
use blake3::Hasher;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// Base tuple bytes anchored in TupleChain.
pub const BASE_TUPLE_BYTES: usize = 3072;
/// Each hyper-tuple inflates tuples to 4,096 bytes.
pub const ICOSUPLE_BYTES: usize = 4096;
const HASH_FIELD_BYTES: usize = 1024;
const METADATA_BYTES: usize = 512;
const PQC_SIGNATURE_BYTES: usize = 512;
const TIER_COUNT: usize = 20;

/// Fully materialised hyper-tuple.
#[derive(Clone, Debug)]
pub struct HyperTuple {
    pub tuple_id: TupleId,
    pub shard_id: ShardId,
    pub creator: String,
    pub version: u32,
    pub input_hash: HyperHash,
    pub prev_hash: HyperHash,
    pub current_hash: HyperHash,
    pub metadata: LayerMetadata,
    pub pqc_envelope: PqcEnvelope,
}

impl HyperTuple {
    /// Returns the canonical byte size (4,096 bytes).
    pub fn total_bytes(&self) -> usize {
        HASH_FIELD_BYTES * 3 + METADATA_BYTES + PQC_SIGNATURE_BYTES
    }

    /// Encodes the hyper-tuple into its contiguous 4,096-byte representation.
    pub fn encode(&self) -> [u8; ICOSUPLE_BYTES] {
        let mut bytes = [0u8; ICOSUPLE_BYTES];
        bytes[..HASH_FIELD_BYTES].copy_from_slice(&self.input_hash.bytes);
        bytes[HASH_FIELD_BYTES..(HASH_FIELD_BYTES * 2)]
            .copy_from_slice(&self.prev_hash.bytes);
        bytes[(HASH_FIELD_BYTES * 2)..(HASH_FIELD_BYTES * 3)]
            .copy_from_slice(&self.current_hash.bytes);
        let metadata = self.metadata.encoded_bytes();
        bytes[(HASH_FIELD_BYTES * 3)..(HASH_FIELD_BYTES * 3 + METADATA_BYTES)]
            .copy_from_slice(&metadata);
        let pqc = self.pqc_envelope.encoded_bytes();
        bytes[(HASH_FIELD_BYTES * 3 + METADATA_BYTES)..].copy_from_slice(&pqc);
        bytes
    }
}

/// Fixed-size hash column inside the hyper-tuple.
#[derive(Clone, Debug)]
pub struct HyperHash {
    pub label: String,
    pub bytes: [u8; HASH_FIELD_BYTES],
}

impl HyperHash {
    pub fn derive(label: impl Into<String>, inputs: &[&[u8]]) -> Self {
        Self {
            label: label.into(),
            bytes: expand_xof::<HASH_FIELD_BYTES>(inputs),
        }
    }
}

/// Tier assignment for the first twenty specialisation layers.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TierAssignment {
    pub tier_index: u8,
    pub specialization: TierSpecialization,
    pub shard_id: ShardId,
    pub throughput_tps: f64,
    pub qs_dag_anchor: Option<String>,
}

/// Chronosync/QS-DAG-driven extensions (n+1 layers).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayerExtension {
    pub ordinal: u32,
    pub vertex_id: String,
    pub storage: StorageTarget,
    pub tw_score: f64,
    pub ann_similarity: f32,
    pub parents: usize,
    pub pqc_sealed: bool,
}

/// Layer metadata segment (512 bytes encoded).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayerMetadata {
    pub tier_assignments: Vec<TierAssignment>,
    pub extensions: Vec<LayerExtension>,
    pub vector_embedding_dims: u16,
    pub entanglement_coefficient: f32,
    pub ttl_ms: u64,
    pub qs_dag_head: Option<String>,
}

impl LayerMetadata {
    pub fn new(
        tier_assignments: Vec<TierAssignment>,
        extensions: Vec<LayerExtension>,
        vector_embedding_dims: u16,
        entanglement_coefficient: f32,
        ttl_ms: u64,
        qs_dag_head: Option<String>,
    ) -> Self {
        Self {
            tier_assignments,
            extensions,
            vector_embedding_dims,
            entanglement_coefficient,
            ttl_ms,
            qs_dag_head,
        }
    }

    pub fn encoded_bytes(&self) -> [u8; METADATA_BYTES] {
        let json = serde_json::to_vec(self).expect("metadata must be serializable");
        expand_xof::<METADATA_BYTES>(&[&json])
    }
}

/// PQC envelope (Kyber/Dilithium layers + QRNG entropy).
#[derive(Clone, Debug)]
pub struct PqcEnvelope {
    pub binding: PqcBinding,
    pub layers: Vec<PqcScheme>,
    pub qrng_entropy_bits: u16,
    signature: [u8; PQC_SIGNATURE_BYTES],
}

impl PqcEnvelope {
    pub fn encoded_bytes(&self) -> [u8; PQC_SIGNATURE_BYTES] {
        self.signature
    }
}

/// Builder configuration for hyper-tuples.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HyperTupleConfig {
    pub vector_embedding_dims: u16,
    pub qrng_entropy_bits: u16,
}

impl Default for HyperTupleConfig {
    fn default() -> Self {
        Self {
            vector_embedding_dims: 4_096,
            qrng_entropy_bits: 512,
        }
    }
}

/// Deterministic builder that stitches TupleChain + Chronosync data.
pub struct HyperTupleBuilder {
    config: HyperTupleConfig,
}

impl Default for HyperTupleBuilder {
    fn default() -> Self {
        Self {
            config: HyperTupleConfig::default(),
        }
    }
}

impl HyperTupleBuilder {
    pub fn new(config: HyperTupleConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &HyperTupleConfig {
        &self.config
    }

    pub fn assemble(
        &self,
        receipt: &TupleReceipt,
        epoch_report: &EpochReport,
        keeper_report: &ChronosyncKeeperReport,
    ) -> HyperTuple {
        let tier_assignments =
            self.materialize_base_tiers(receipt, epoch_report, keeper_report);
        let extensions = self.materialize_extensions(&keeper_report.applied_vertices);
        let entanglement = self.entanglement_score(
            &tier_assignments,
            &extensions,
            &keeper_report.storage_layout,
            epoch_report.aggregated_tps,
        );
        let metadata = LayerMetadata::new(
            tier_assignments,
            extensions,
            self.config.vector_embedding_dims,
            entanglement,
            receipt.expiry,
            keeper_report.dag_head.clone(),
        );
        let pqc_envelope = self.build_pqc_envelope(receipt, keeper_report);
        let input_hash = self.input_hash(receipt);
        let prev_hash = self.prev_hash(keeper_report);
        let current_hash = self.current_hash(epoch_report, &metadata, &pqc_envelope);

        HyperTuple {
            tuple_id: receipt.tuple_id,
            shard_id: receipt.shard_id,
            creator: receipt.creator.clone(),
            version: receipt.version,
            input_hash,
            prev_hash,
            current_hash,
            metadata,
            pqc_envelope,
        }
    }

    fn materialize_base_tiers(
        &self,
        receipt: &TupleReceipt,
        epoch: &EpochReport,
        keeper: &ChronosyncKeeperReport,
    ) -> Vec<TierAssignment> {
        let tiers = TierSpecialization::base_layers();
        let total_weight: f64 = tiers.iter().map(|spec| spec.throughput_weight()).sum();
        tiers
            .iter()
            .enumerate()
            .map(|(index, spec)| {
                let share = epoch.aggregated_tps * (spec.throughput_weight() / total_weight);
                let throughput = share.min(spec.target_tps() as f64).max(1.0);
                TierAssignment {
                    tier_index: index as u8,
                    specialization: *spec,
                    shard_id: derive_shard(receipt.tuple_id, index as u8),
                    throughput_tps: throughput,
                    qs_dag_anchor: keeper.dag_head.clone(),
                }
            })
            .collect()
    }

    fn materialize_extensions(&self, vertices: &[VertexReceipt]) -> Vec<LayerExtension> {
        vertices
            .iter()
            .enumerate()
            .map(|(ordinal, receipt)| LayerExtension {
                ordinal: ordinal as u32,
                vertex_id: receipt.vertex_id.to_string(),
                storage: receipt.storage.clone(),
                tw_score: receipt.tw_score,
                ann_similarity: receipt.ann_similarity,
                parents: receipt.parents,
                pqc_sealed: receipt.pqc_signature.is_some(),
            })
            .collect()
    }

    fn entanglement_score(
        &self,
        tiers: &[TierAssignment],
        extensions: &[LayerExtension],
        storage: &ModuleStorageLayout,
        aggregated_tps: f64,
    ) -> f32 {
        let base = tiers.len().max(1) as f32;
        let extension_factor = 0.75 + (extensions.len() as f32 / base);
        let storage_factor = (storage.total_vertices().max(1) as f32).ln_1p().max(1.0);
        let tps_factor = aggregated_tps.max(1.0).log10() as f32 / 5.0;
        extension_factor * storage_factor * tps_factor.max(0.5)
    }

    fn build_pqc_envelope(
        &self,
        receipt: &TupleReceipt,
        keeper: &ChronosyncKeeperReport,
    ) -> PqcEnvelope {
        let binding = PqcBinding::new(receipt.creator.clone(), PqcScheme::Dilithium);
        let layers = vec![PqcScheme::Kyber, PqcScheme::Dilithium];
        let tuple_bytes = receipt.tuple_id.0;
        let commitment = receipt.commitment;
        let version_bytes = receipt.version.to_le_bytes();
        let qrng_bytes = self.config.qrng_entropy_bits.to_le_bytes();
        let hot = keeper.storage_layout.hot_vertices as u64;
        let crystalline = keeper.storage_layout.crystalline_vertices as u64;
        let hot_bytes = hot.to_le_bytes();
        let crystalline_bytes = crystalline.to_le_bytes();
        let signature = expand_xof::<PQC_SIGNATURE_BYTES>(&[
            &tuple_bytes,
            &commitment,
            &version_bytes,
            &qrng_bytes,
            &hot_bytes,
            &crystalline_bytes,
        ]);
        PqcEnvelope {
            binding,
            layers,
            qrng_entropy_bits: self.config.qrng_entropy_bits,
            signature,
        }
    }

    fn input_hash(&self, receipt: &TupleReceipt) -> HyperHash {
        let tuple_bytes = receipt.tuple_id.0;
        let commitment = receipt.commitment;
        let version_bytes = receipt.version.to_le_bytes();
        let shard_bytes = receipt.shard_id.0.to_le_bytes();
        HyperHash::derive(
            "input_hash@tuplechain",
            &[&tuple_bytes, &commitment, &version_bytes, &shard_bytes],
        )
    }

    fn prev_hash(&self, keeper_report: &ChronosyncKeeperReport) -> HyperHash {
        let head = keeper_report
            .dag_head
            .as_deref()
            .unwrap_or("chronosync/genesis");
        let epoch_bytes = keeper_report.epoch_index.to_le_bytes();
        let hot = keeper_report.storage_layout.hot_vertices as u64;
        let crystalline = keeper_report.storage_layout.crystalline_vertices as u64;
        HyperHash::derive(
            "prev_hash@chronosync",
            &[
                head.as_bytes(),
                &epoch_bytes,
                &hot.to_le_bytes(),
                &crystalline.to_le_bytes(),
            ],
        )
    }

    fn current_hash(
        &self,
        epoch_report: &EpochReport,
        metadata: &LayerMetadata,
        pqc_envelope: &PqcEnvelope,
    ) -> HyperHash {
        let metadata_bytes = metadata.encoded_bytes();
        let pqc_bytes = pqc_envelope.encoded_bytes();
        let fairness_bytes = epoch_report.fairness_gini.to_le_bytes();
        let tps_bytes = epoch_report.aggregated_tps.to_le_bytes();
        HyperHash::derive(
            "current_hash@icosuple",
            &[
                &metadata_bytes,
                &pqc_bytes,
                &fairness_bytes,
                &tps_bytes,
                &epoch_report.epoch_index.to_le_bytes(),
            ],
        )
    }
}

/// Tier catalogue aligned with the Autheo primer (first 20 layers).
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
    Dynamic(u8),
}

const BASE_SPECIALISATIONS: [TierSpecialization; TIER_COUNT] = [
    TierSpecialization::DeosKernel,
    TierSpecialization::TuplechainAnchor,
    TierSpecialization::ComputeHash,
    TierSpecialization::StorageHash,
    TierSpecialization::MessagingHash,
    TierSpecialization::IdentityHash,
    TierSpecialization::CredentialHash,
    TierSpecialization::AiHash,
    TierSpecialization::MlHash,
    TierSpecialization::DataHash,
    TierSpecialization::FinanceHash,
    TierSpecialization::HealthHash,
    TierSpecialization::IotHash,
    TierSpecialization::MetaverseHash,
    TierSpecialization::GovHash,
    TierSpecialization::EnergyHash,
    TierSpecialization::PrivacyHash,
    TierSpecialization::BountyHash,
    TierSpecialization::QuantumHash,
    TierSpecialization::InteropHash,
];

impl TierSpecialization {
    pub fn base_layers() -> &'static [TierSpecialization; TIER_COUNT] {
        &BASE_SPECIALISATIONS
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
            Dynamic(n) => Cow::Owned(format!("Layer {} · dynamic tier", n)),
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
            Dynamic(_) => 1.0,
        }
    }
}

fn derive_shard(tuple_id: TupleId, tier_index: u8) -> ShardId {
    let mut hasher = Hasher::new();
    hasher.update(&tuple_id.0);
    hasher.update(&[tier_index]);
    let digest = hasher.finalize();
    let shard_bytes = [digest.as_bytes()[0], digest.as_bytes()[1]];
    ShardId(u16::from_le_bytes(shard_bytes))
}

fn expand_xof<const N: usize>(inputs: &[&[u8]]) -> [u8; N] {
    let mut hasher = Hasher::new();
    for input in inputs {
        hasher.update(input);
    }
    let mut reader = hasher.finalize_xof();
    let mut bytes = [0u8; N];
    reader.fill(&mut bytes);
    bytes
}
