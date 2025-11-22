//! TupleChain semantic ledger primitives for Autheo PQCNet.
//!
//! The crate models the five-element tuple `(subject, predicate, object, proof, expiry)`
//! together with shard assignments, versioned history, and keeper-friendly APIs that slot directly
//! into Cosmos SDK, WASM, or native runtimes.

use blake3::Hasher;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Identifier assigned to every tuple stored in the ledger.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TupleId(pub [u8; 32]);

impl TupleId {
    /// Hex-encoded representation of the identifier.
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}

impl fmt::Display for TupleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// Logical shard identifier derived from tuple metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ShardId(pub u16);

impl fmt::Display for ShardId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "shard-{}", self.0)
    }
}

const TYPE_URL_SUBJECT: &str = "autheo.tuplechain.v1.Subject";
const TYPE_URL_PREDICATE: &str = "autheo.tuplechain.v1.Predicate";
const TYPE_URL_OBJECT: &str = "autheo.tuplechain.v1.Object";
const TYPE_URL_PROOF: &str = "autheo.tuplechain.v1.ProofEnvelope";

/// Lightweight representation of `google.protobuf.Any`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TupleAny {
    pub type_url: String,
    pub value: Vec<u8>,
}

impl TupleAny {
    pub fn new(type_url: impl Into<String>, value: impl Into<Vec<u8>>) -> Self {
        Self {
            type_url: type_url.into(),
            value: value.into(),
        }
    }

    pub fn from_string(type_url: impl Into<String>, value: impl Into<String>) -> Self {
        Self::new(type_url, value.into().into_bytes())
    }

    pub fn subject(value: impl Into<String>) -> Self {
        Self::from_string(TYPE_URL_SUBJECT, value)
    }

    pub fn predicate(value: impl Into<String>) -> Self {
        Self::from_string(TYPE_URL_PREDICATE, value)
    }

    pub fn object(value: &Value) -> Self {
        let bytes = serde_json::to_vec(value).unwrap_or_default();
        Self::new(TYPE_URL_OBJECT, bytes)
    }

    pub fn proof_envelope(envelope: &ProofEnvelope) -> Self {
        let bytes = serde_json::to_vec(envelope).unwrap_or_default();
        Self::new(TYPE_URL_PROOF, bytes)
    }

    pub fn len(&self) -> usize {
        self.type_url.len() + self.value.len()
    }
}

/// Supported proof primitives for TupleChain tuples.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProofScheme {
    /// Zero-knowledge circuits (Groth16, PLONK, etc.).
    Zkp,
    /// Fully homomorphic encryption snapshots.
    Fhe,
    /// Signature-based attestations (Dilithium, Falcon, etc.).
    Signature,
    /// Custom scheme identifier.
    Custom(String),
}

/// Proof metadata embedded in every tuple.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProofEnvelope {
    pub scheme: ProofScheme,
    pub commitment: [u8; 32],
    pub verifier_hint: String,
}

impl ProofEnvelope {
    /// Construct a new proof envelope by hashing the transcript bytes.
    pub fn new(
        scheme: ProofScheme,
        transcript: impl AsRef<[u8]>,
        verifier_hint: impl Into<String>,
    ) -> Self {
        let mut hasher = Hasher::new();
        hasher.update(transcript.as_ref());
        let mut commitment = [0u8; 32];
        commitment.copy_from_slice(hasher.finalize().as_bytes());
        Self {
            scheme,
            commitment,
            verifier_hint: verifier_hint.into(),
        }
    }

    fn placeholder() -> Self {
        Self::new(
            ProofScheme::Custom("placeholder".into()),
            b"pending-proof",
            "placeholder",
        )
    }
}

/// Canonical TupleChain payload backed by repeated `Any` data fields.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TuplePayload {
    data: Vec<TupleAny>,
    pub proof: ProofEnvelope,
    pub expiry: u64,
}

impl TuplePayload {
    /// Start a builder for a tuple.
    pub fn builder(subject: impl Into<String>, predicate: impl Into<String>) -> TupleBuilder {
        TupleBuilder::new(subject, predicate)
    }

    /// Access the underlying Any fields.
    pub fn data(&self) -> &[TupleAny] {
        &self.data
    }

    fn approx_size(&self) -> usize {
        self.data
            .iter()
            .map(TupleAny::len)
            .sum::<usize>()
            + self.proof.verifier_hint.len()
            + self.proof.commitment.len()
            + std::mem::size_of::<u64>()
    }
}

/// Fluent tuple builder used by demos/tests and Cosmos SDK handlers.
pub struct TupleBuilder {
    subject: String,
    predicate: String,
    object: Value,
    proof: ProofEnvelope,
    expiry: u64,
    extras: Vec<TupleAny>,
}

impl TupleBuilder {
    fn new(subject: impl Into<String>, predicate: impl Into<String>) -> Self {
        Self {
            subject: subject.into(),
            predicate: predicate.into(),
            object: Value::Null,
            proof: ProofEnvelope::placeholder(),
            expiry: u64::MAX,
            extras: Vec::new(),
        }
    }

    pub fn object_value(mut self, value: Value) -> Self {
        self.object = value;
        self
    }

    pub fn object_text(mut self, value: impl Into<String>) -> Self {
        self.object = Value::String(value.into());
        self
    }

    pub fn proof(
        mut self,
        scheme: ProofScheme,
        transcript: impl AsRef<[u8]>,
        hint: impl Into<String>,
    ) -> Self {
        self.proof = ProofEnvelope::new(scheme, transcript, hint);
        self
    }

    pub fn expiry(mut self, expiry_ms: u64) -> Self {
        self.expiry = expiry_ms;
        self
    }

    /// Attach an arbitrary Any field to the tuple payload.
    pub fn add_any(mut self, field: TupleAny) -> Self {
        self.extras.push(field);
        self
    }

    pub fn build(self) -> TuplePayload {
        let mut data = Vec::with_capacity(3 + self.extras.len());
        data.push(TupleAny::subject(self.subject));
        data.push(TupleAny::predicate(self.predicate));
        data.push(TupleAny::object(&self.object));
        data.extend(self.extras);
        TuplePayload {
            data,
            proof: self.proof,
            expiry: self.expiry,
        }
    }
}

/// Configuration for TupleChain shards and historical retention.
#[derive(Clone, Debug)]
pub struct TupleChainConfig {
    pub shard_count: u16,
    pub max_tuple_size: usize,
    pub max_historical_versions: usize,
    pub sharding_threshold: usize,
}

impl Default for TupleChainConfig {
    fn default() -> Self {
        Self {
            shard_count: 32,
            max_tuple_size: 4096,
            max_historical_versions: 8,
            sharding_threshold: 100_000,
        }
    }
}

/// Errors raised by TupleChain operations.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum TupleChainError {
    #[error("tuple exceeds max size: {size}B (max {limit}B)")]
    TupleTooLarge { size: usize, limit: usize },
    #[error("creator {creator} is not authorized to store tuples")]
    UnauthorizedCreator { creator: String },
    #[error("creator mismatch: expected {expected}, received {actual}")]
    CreatorMismatch { expected: String, actual: String },
    #[error("tuple not found")]
    TupleNotFound,
    #[error("version {version} not found for tuple")]
    VersionNotFound { version: u32 },
    #[error("tuple already deleted")]
    TupleAlreadyDeleted,
}

/// Lifecycle status of a tuple version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TupleStatus {
    Active,
    Historical,
    Expired,
    Deleted,
}

/// Materialized tuple version stored in a shard timeline.
#[derive(Clone, Debug)]
pub struct TupleVersionedRecord {
    pub version: u32,
    pub tuple: TuplePayload,
    pub status: TupleStatus,
    pub commitment: [u8; 32],
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug)]
struct TupleTimeline {
    shard_id: ShardId,
    creator: String,
    records: Vec<TupleVersionedRecord>,
}

impl TupleTimeline {
    fn new(shard_id: ShardId, creator: String) -> Self {
        Self {
            shard_id,
            creator,
            records: Vec::new(),
        }
    }

    fn latest_active(&self) -> Option<&TupleVersionedRecord> {
        self.records
            .iter()
            .rev()
            .find(|record| record.status == TupleStatus::Active)
    }

    fn head(&self) -> Option<&TupleVersionedRecord> {
        self.records.last()
    }
}

/// Fully materialized tuple snapshot used by queries and gRPC surfaces.
#[derive(Clone, Debug)]
pub struct TupleSnapshot {
    pub tuple_id: TupleId,
    pub shard_id: ShardId,
    pub creator: String,
    pub version: u32,
    pub tuple: TuplePayload,
    pub status: TupleStatus,
    pub commitment: [u8; 32],
    pub timestamp_ms: u64,
}

#[derive(Clone, Default, Debug)]
struct ShardStats {
    active: usize,
    historical: usize,
    expired: usize,
    deleted: usize,
    bytes: usize,
}

/// 20-tier sharding model (compressed to three tiers for ergonomics).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IcosupleTier {
    Base,
    Mid,
    Apex,
}

impl IcosupleTier {
    pub const PATH: [Self; 3] = [Self::Base, Self::Mid, Self::Apex];
}

/// Receipt returned after storing tuple data.
#[derive(Clone, Debug)]
pub struct TupleReceipt {
    pub tuple_id: TupleId,
    pub shard_id: ShardId,
    pub version: u32,
    pub commitment: [u8; 32],
    pub tier_path: [IcosupleTier; 3],
    pub expiry: u64,
    pub creator: String,
    pub timestamp_ms: u64,
}

/// Ledger implementation supporting sharded versioned tuples.
pub struct TupleChainLedger {
    config: TupleChainConfig,
    timelines: BTreeMap<TupleId, TupleTimeline>,
    shard_stats: Vec<ShardStats>,
}

impl TupleChainLedger {
    pub fn new(config: TupleChainConfig) -> Self {
        let shard_stats = vec![ShardStats::default(); config.shard_count as usize];
        Self {
            config,
            timelines: BTreeMap::new(),
            shard_stats,
        }
    }

    pub fn config(&self) -> &TupleChainConfig {
        &self.config
    }

    pub fn store_tuple(
        &mut self,
        tuple: TuplePayload,
        now_ms: u64,
        creator: &str,
    ) -> Result<TupleReceipt, TupleChainError> {
        self.store_tuple_with_shard_hint(tuple, now_ms, creator, None)
    }

    pub fn store_tuple_with_shard_hint(
        &mut self,
        tuple: TuplePayload,
        now_ms: u64,
        creator: &str,
        shard_hint: Option<ShardId>,
    ) -> Result<TupleReceipt, TupleChainError> {
        let size = tuple.approx_size();
        if size > self.config.max_tuple_size {
            return Err(TupleChainError::TupleTooLarge {
                size,
                limit: self.config.max_tuple_size,
            });
        }

        let tuple_id = compute_tuple_id(&tuple);
        let shard_count = self.config.shard_count as usize;
        let mut target_index = shard_hint
            .map(|hint| hint.0 as usize)
            .filter(|idx| *idx < shard_count)
            .unwrap_or_else(|| self.assign_shard(&tuple));

        let (timeline, shard_id) = if let Some(timeline) = self.timelines.get_mut(&tuple_id) {
            if timeline.creator != creator {
                return Err(TupleChainError::CreatorMismatch {
                    expected: timeline.creator.clone(),
                    actual: creator.into(),
                });
            }
            let shard_id = timeline.shard_id;
            (timeline, shard_id)
        } else {
            if self.shard_over_threshold(target_index) {
                target_index = self.lowest_load_shard();
            }
            let shard_id = ShardId(target_index as u16);
            let timeline = self
                .timelines
                .entry(tuple_id)
                .or_insert_with(|| TupleTimeline::new(shard_id, creator.to_string()));
            (timeline, shard_id)
        };

        if matches!(
            timeline.head(),
            Some(record) if record.status == TupleStatus::Deleted
        ) {
            return Err(TupleChainError::TupleAlreadyDeleted);
        }

        for record in &mut timeline.records {
            if record.status == TupleStatus::Active {
                record.status = TupleStatus::Historical;
            }
        }

        let version = timeline.records.last().map(|r| r.version + 1).unwrap_or(1);
        let commitment = commit_tuple(&tuple_id, &tuple, creator, version, now_ms);

        let record = TupleVersionedRecord {
            version,
            tuple: tuple.clone(),
            status: TupleStatus::Active,
            commitment,
            timestamp_ms: now_ms,
        };
        timeline.records.push(record);

        if timeline.records.len() > self.config.max_historical_versions {
            timeline.records.remove(0);
        }

        self.rebuild_shard_stats(shard_id.0 as usize);

        Ok(TupleReceipt {
            tuple_id,
            shard_id,
            version,
            commitment,
            tier_path: IcosupleTier::PATH,
            expiry: tuple.expiry,
            creator: creator.into(),
            timestamp_ms: now_ms,
        })
    }

    pub fn latest(&self, tuple_id: &TupleId) -> Option<&TupleVersionedRecord> {
        self.timelines
            .get(tuple_id)
            .and_then(TupleTimeline::latest_active)
    }

    pub fn by_version(
        &self,
        tuple_id: &TupleId,
        version: u32,
    ) -> Result<&TupleVersionedRecord, TupleChainError> {
        let timeline = self
            .timelines
            .get(tuple_id)
            .ok_or(TupleChainError::TupleNotFound)?;
        timeline
            .records
            .iter()
            .find(|record| record.version == version)
            .ok_or(TupleChainError::VersionNotFound { version })
    }

    pub fn prune_expired(&mut self, now_ms: u64) -> Vec<TupleId> {
        let mut expired_ids = Vec::new();
        let mut shards_to_rebuild = BTreeSet::new();

        for (tuple_id, timeline) in self.timelines.iter_mut() {
            let mut changed = false;
            for record in &mut timeline.records {
                if record.status != TupleStatus::Expired
                    && record.status != TupleStatus::Deleted
                    && record.tuple.expiry <= now_ms
                {
                    record.status = TupleStatus::Expired;
                    changed = true;
                }
            }
            if changed {
                expired_ids.push(*tuple_id);
                shards_to_rebuild.insert(timeline.shard_id.0 as usize);
            }
        }

        for shard_index in shards_to_rebuild {
            self.rebuild_shard_stats(shard_index);
        }

        expired_ids
    }

    pub fn shard_utilization(&self) -> Vec<ShardUtilization> {
        let mut utilization = Vec::with_capacity(self.shard_stats.len() * 3);
        for (index, stats) in self.shard_stats.iter().enumerate() {
            let base_capacity = (self.config.max_tuple_size * 64).max(1);
            let base_load = (stats.bytes as f32 / base_capacity as f32).min(1.0);
            let shard_id = ShardId(index as u16);

            utilization.push(ShardUtilization {
                shard_id,
                tier: IcosupleTier::Base,
                tuples: stats.active,
                bytes: stats.bytes,
                load_factor: base_load,
            });
            utilization.push(ShardUtilization {
                shard_id,
                tier: IcosupleTier::Mid,
                tuples: stats.active + stats.historical,
                bytes: stats.bytes / 2 + stats.historical * 32,
                load_factor: (base_load * 0.7).min(1.0),
            });
            utilization.push(ShardUtilization {
                shard_id,
                tier: IcosupleTier::Apex,
                tuples: stats.active + stats.historical + stats.expired + stats.deleted,
                bytes: stats.bytes / 4 + stats.expired * 16 + stats.deleted * 8,
                load_factor: (base_load * 0.4).min(1.0),
            });
        }
        utilization
    }

    fn assign_shard(&self, tuple: &TuplePayload) -> usize {
        let mut hasher = Hasher::new();
        for field in tuple.data().iter().take(3) {
            hasher.update(field.type_url.as_bytes());
            hasher.update(&field.value);
        }
        hasher.update(&tuple.proof.commitment);
        let digest = hasher.finalize();
        let mut shard_bytes = [0u8; 8];
        shard_bytes.copy_from_slice(&digest.as_bytes()[..8]);
        let shard_seed = u64::from_le_bytes(shard_bytes);
        (shard_seed as usize) % self.config.shard_count as usize
    }

    fn shard_over_threshold(&self, shard_index: usize) -> bool {
        self.shard_stats
            .get(shard_index)
            .map(|stats| stats.active >= self.config.sharding_threshold)
            .unwrap_or(false)
    }

    fn lowest_load_shard(&self) -> usize {
        let mut best_idx = 0;
        let mut best_load = usize::MAX;
        for (idx, stats) in self.shard_stats.iter().enumerate() {
            if stats.active < best_load {
                best_load = stats.active;
                best_idx = idx;
            }
        }
        best_idx
    }

    fn rebuild_shard_stats(&mut self, shard_index: usize) {
        let mut stats = ShardStats::default();
        for timeline in self.timelines.values() {
            if timeline.shard_id.0 as usize != shard_index {
                continue;
            }
            for record in &timeline.records {
                match record.status {
                    TupleStatus::Active => stats.active += 1,
                    TupleStatus::Historical => stats.historical += 1,
                    TupleStatus::Expired => stats.expired += 1,
                    TupleStatus::Deleted => stats.deleted += 1,
                }
                stats.bytes += record.tuple.approx_size();
            }
        }
        if let Some(slot) = self.shard_stats.get_mut(shard_index) {
            *slot = stats;
        }
    }
}

fn compute_tuple_id(tuple: &TuplePayload) -> TupleId {
    let mut hasher = Hasher::new();
    for field in tuple.data() {
        hasher.update(field.type_url.as_bytes());
        hasher.update(&field.value);
    }
    hasher.update(&tuple.proof.commitment);
    let mut id = [0u8; 32];
    id.copy_from_slice(hasher.finalize().as_bytes());
    TupleId(id)
}

fn commit_tuple(
    tuple_id: &TupleId,
    tuple: &TuplePayload,
    creator: &str,
    version: u32,
    now_ms: u64,
) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(&tuple_id.0);
    for field in tuple.data() {
        hasher.update(field.type_url.as_bytes());
        hasher.update(&field.value);
    }
    hasher.update(&tuple.proof.commitment);
    hasher.update(creator.as_bytes());
    hasher.update(&version.to_le_bytes());
    hasher.update(&now_ms.to_le_bytes());
    let mut commitment = [0u8; 32];
    commitment.copy_from_slice(hasher.finalize().as_bytes());
    commitment
}

/// Snapshot of shard load used for demos/telemetry.
#[derive(Clone, Debug)]
pub struct ShardUtilization {
    pub shard_id: ShardId,
    pub tier: IcosupleTier,
    pub tuples: usize,
    pub bytes: usize,
    pub load_factor: f32,
}

/// Keeper façade that mimics a Cosmos SDK module keeper.
pub struct TupleChainKeeper {
    ledger: TupleChainLedger,
    allowed_creators: BTreeSet<String>,
}

impl TupleChainKeeper {
    pub fn new(config: TupleChainConfig) -> Self {
        Self {
            ledger: TupleChainLedger::new(config),
            allowed_creators: BTreeSet::new(),
        }
    }

    pub fn allow_creator(mut self, creator: impl Into<String>) -> Self {
        self.allowed_creators.insert(creator.into());
        self
    }

    pub fn register_creator(&mut self, creator: impl Into<String>) {
        self.allowed_creators.insert(creator.into());
    }

    pub fn store_tuple(
        &mut self,
        creator: &str,
        tuple: TuplePayload,
        now_ms: u64,
    ) -> Result<TupleReceipt, TupleChainError> {
        if !self.allowed_creators.is_empty() && !self.allowed_creators.contains(creator) {
            return Err(TupleChainError::UnauthorizedCreator {
                creator: creator.into(),
            });
        }
        self.ledger.store_tuple(tuple, now_ms, creator)
    }

    pub fn ledger(&self) -> &TupleChainLedger {
        &self.ledger
    }

    pub fn ledger_mut(&mut self) -> &mut TupleChainLedger {
        &mut self.ledger
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo_tuple(expiry_offset: u64) -> TuplePayload {
        TuplePayload::builder("did:autheo:alice", "owns")
            .object_text("decredential")
            .proof(ProofScheme::Zkp, b"proof", "zkp")
            .expiry(1_700_000_000_000 + expiry_offset)
            .build()
    }

    #[test]
    fn ledger_stores_and_versions() {
        let config = TupleChainConfig::default();
        let mut ledger = TupleChainLedger::new(config);
        let creator = "did:autheo:l1/kernel";

        let first = ledger
            .store_tuple(demo_tuple(10_000), 1_700_000_000_000, creator)
            .expect("store tuple");
        let second = ledger
            .store_tuple(demo_tuple(20_000), 1_700_000_010_000, creator)
            .expect("second version");

        assert_eq!(first.tuple_id, second.tuple_id);
        assert_eq!(second.version, first.version + 1);
        assert!(ledger.latest(&first.tuple_id).is_some());
    }

    #[test]
    fn keeper_respects_authorization() {
        let mut keeper =
            TupleChainKeeper::new(TupleChainConfig::default()).allow_creator("creator-a");
        let tuple = demo_tuple(5_000);
        assert!(keeper
            .store_tuple("creator-a", tuple.clone(), 1_700_000_000_000)
            .is_ok());
        let err = keeper
            .store_tuple("creator-b", tuple, 1_700_000_000_000)
            .unwrap_err();
        assert!(matches!(err, TupleChainError::UnauthorizedCreator { .. }));
    }
}
