use autheo_pqcnet_5dqeh::{
    HypergraphState, Icosuple as QehIcosuple, ModuleStorageLayout, QehConfig,
    TemporalWeightInput as HyperTemporalWeightInput, TemporalWeightModel, VertexReceipt,
};
use autheo_pqcnet_chronosync::{
    ChronosyncKeeperReport, DagNode, DagWitness, EpochReport, NodeSelection, ShardLoad,
    TierDesignation, VerificationPoolSnapshot,
};
use autheo_pqcnet_icosuple::{HyperTupleBuilder, HyperTupleConfig, ICOSUPLE_BYTES as TIER_BYTES};
use autheo_pqcnet_qrng::{EntropyRequest, QrngSim};
use autheo_pqcnet_tuplechain::{IcosupleTier, ShardId, TupleId, TupleReceipt};
use pqcnet_qs_dag::{
    DagError, HierarchicalVerificationPools, IcosupleLayer, PayloadProfile, PoolTier, QIPTag,
    QrngCore, QsDag, ShardManager, ShardPolicy, StateDiff, StateOp, StateSnapshot, TemporalWeight,
    TupleDomain, TupleEnvelope, TupleValidation, VerificationVerdict,
};
use serde::Serialize;
use std::f64::consts::PI;
use std::fs;
use std::path::PathBuf;

struct DemoQrng(u64);

impl QrngCore for DemoQrng {
    type Error = core::convert::Infallible;

    fn next_u64(&mut self) -> Result<u64, Self::Error> {
        let current = self.0;
        self.0 = self.0.wrapping_add(1);
        Ok(current)
    }
}

fn main() -> Result<(), DagError> {
    let genesis = StateDiff::genesis("genesis", "bootstrap");
    let mut dag = QsDag::with_temporal_weight(genesis, TemporalWeight::new(32))?;

    let tuple = TupleEnvelope::new(
        TupleDomain::Finance,
        IcosupleLayer::CONSENSUS_TIER_9,
        PayloadProfile::AssetTransfer,
        "did:finance:alpha",
        "did:finance:beta",
        1_000_000,
        b"zk-settlement v1",
        [0xA5; 32],
        1_713_861_234_112,
        QIPTag::Bridge("QIP:Solana".to_string()),
        None,
        TupleValidation::new("Dilithium5", vec![0; 64], vec![1; 64]),
    )
    .without_inline_payload();

    let settlement_diff = StateDiff::with_tuple(
        "tuple-finance-001",
        "validator-alpha",
        vec!["genesis".into()],
        1,
        vec![
            StateOp::upsert("finance/routes/solana", "bridge-online"),
            StateOp::upsert("finance/latency-ms", "2.4"),
        ],
        tuple,
    );
    dag.insert(settlement_diff.clone())?;

    // Hierarchical verification pools: 2 tiers, QRNG-elected coordinators.
    let mut hvp = HierarchicalVerificationPools::new(
        vec![PoolTier::new(8, 4), PoolTier::new(9, 4)],
        DemoQrng(42),
    );
    for validator in ["alice", "bob", "carol", "dave"] {
        hvp.register_validator(8, validator.to_string());
    }
    hvp.register_validator(9, "eve".to_string());
    hvp.register_validator(9, "frank".to_string());
    let coordinators = hvp.elect_coordinators().expect("qrng never fails");
    println!("QRNG coordinators: {coordinators:?}");
    for validator in ["alice", "bob", "carol", "dave"] {
        if let Some(outcome) = hvp.submit_vote(
            settlement_diff.id.clone(),
            &validator.to_string(),
            VerificationVerdict::Approve,
        ) {
            println!(
                "Finalized tuple {} with verdict {:?}",
                outcome.tuple_id, outcome.verdict
            );
            break;
        }
    }

    // Dynamic tuple sharding per domain.
    let mut shards = ShardManager::new(ShardPolicy::new(2));
    let assignment = shards
        .assign(settlement_diff.clone())
        .expect("shard assignment succeeds");
    println!(
        "Tuple {} routed to shard {:?} with anchor {:02x?}",
        assignment.tuple_id,
        assignment.shard_id,
        &assignment.global_anchor[..4]
    );

    let snapshot = dag.snapshot().expect("reachable head");
    println!("Canonical head: {}", snapshot.head_id);
    for (key, value) in snapshot.values.iter() {
        println!("{key} => {value}");
    }

    let bridge = build_chsh_bridge(&snapshot);
    let bridge_path = export_chsh_bridge(&bridge);
    println!(
        "\nCHSH bridge exported to {} (epoch {} · {} bits)",
        bridge_path.display(),
        bridge.qrng_epoch,
        bridge.qrng_bits
    );
    println!(
        "Run `python quantum/chsh_sandbox.py --settings {} --shots 4096` \
        to reproduce the two-qubit and 5D violations with QuTiP.",
        bridge_path.display()
    );

    Ok(())
}

fn build_chsh_bridge(snapshot: &StateSnapshot) -> ChshBridgeSnapshot {
    let mut qrng = QrngSim::new(0x5d5d_c357);
    let request = EntropyRequest::for_icosuple(
        "qs-dag-chsh",
        3_072,
        format!("icosuple/{}", snapshot.head_id),
    )
    .with_security(5, 5);
    let frame = qrng
        .run_epoch(&[request])
        .frames
        .into_iter()
        .next()
        .expect("qrng frame");
    let mut cursor = EntropyCursor::new(&frame.entropy);

    let tuple_receipt = synthesize_tuple_receipt(&frame);
    let builder_cfg = HyperTupleConfig::default();
    let vertex_ctx = synthesize_vertex(&frame, &builder_cfg);
    let epoch_report = synthesize_epoch_report(&frame, &tuple_receipt, &vertex_ctx.receipt);
    let keeper_report = ChronosyncKeeperReport {
        epoch_index: frame.epoch,
        applied_vertices: vec![vertex_ctx.receipt.clone()],
        missing_parents: Vec::new(),
        storage_layout: vertex_ctx.storage_layout.clone(),
        dag_head: Some(snapshot.head_id.clone()),
    };

    let builder = HyperTupleBuilder::new(builder_cfg.clone());
    let hyper_tuple = builder.assemble(&tuple_receipt, &epoch_report, &keeper_report);
    let encoded = hyper_tuple.encode();
    let hyper_tuple_hash = blake3::hash(&encoded).to_hex().to_string();

    let two_qubit = derive_two_qubit_plan(&mut cursor);
    let (axes, hyperedges) = derive_five_d_plan(&mut cursor, 5);

    ChshBridgeSnapshot {
        qrng_epoch: frame.epoch,
        qrng_seed_hex: frame.as_hex_seed().chars().take(64).collect(),
        qrng_bits: frame.entropy_bits(),
        tuple_receipt: TupleReceiptDigest::from(&tuple_receipt),
        hyper_tuple_hash,
        hyper_tuple_bits: encoded.len() * 8,
        vertex: VertexReceiptDigest::from(&vertex_ctx.receipt),
        two_qubit,
        five_d: FiveDPlan {
            axes,
            dims: 5,
            target_violation: 2.0 * 2f64.powf(5.0 / 2.0),
        },
        hyperedges,
        dag_state: DagStateDigest::from(snapshot),
    }
}

fn synthesize_tuple_receipt(frame: &autheo_pqcnet_qrng::QrngEntropyFrame) -> TupleReceipt {
    let mut tuple_id = [0u8; 32];
    tuple_id.copy_from_slice(&frame.checksum);
    let timestamp_ms = (frame.timestamp_ps / 1_000).min(u128::from(u64::MAX)) as u64;
    TupleReceipt {
        tuple_id: TupleId(tuple_id),
        shard_id: ShardId((frame.sequence % 1_000) as u16),
        version: 1,
        commitment: frame.checksum,
        tier_path: IcosupleTier::PATH,
        expiry: timestamp_ms.saturating_add(86_400_000),
        creator: format!("tuplechain/{}", frame.request.label),
        timestamp_ms,
    }
}

struct VertexContext {
    receipt: VertexReceipt,
    storage_layout: ModuleStorageLayout,
}

fn synthesize_vertex(
    frame: &autheo_pqcnet_qrng::QrngEntropyFrame,
    cfg: &HyperTupleConfig,
) -> VertexContext {
    let mut qeh_cfg = QehConfig::default();
    qeh_cfg.vector_dimensions = cfg.vector_embedding_dims as usize;
    let mut state = HypergraphState::new(qeh_cfg.clone());
    let model = TemporalWeightModel::default();
    let label = format!("icosuple::chsh/{}", frame.request.icosuple_reference);
    let icosuple = QehIcosuple::synthesize(&qeh_cfg, label, TIER_BYTES, 0.82);
    let tw_input = HyperTemporalWeightInput::new(
        (frame.timestamp_ps / 1_000).min(u128::from(u64::MAX)) as u64,
        0.92,
        frame.envelope.qrng_entropy_bits,
        0.78,
        0.81,
        0.66,
    );
    let receipt = state
        .insert(icosuple, Vec::new(), &model, tw_input, None)
        .expect("hypergraph insertion");
    let mut storage_layout = ModuleStorageLayout::default();
    storage_layout.register(&receipt);
    VertexContext {
        receipt,
        storage_layout,
    }
}

fn synthesize_epoch_report(
    frame: &autheo_pqcnet_qrng::QrngEntropyFrame,
    receipt: &TupleReceipt,
    vertex: &VertexReceipt,
) -> EpochReport {
    let transactions = (vertex.tw_score * 1_000_000.0) as u64 + frame.sequence;
    EpochReport {
        epoch_index: frame.epoch,
        aggregated_tps: 1_050_000_000.0,
        fairness_gini: 0.18,
        pools: vec![VerificationPoolSnapshot {
            pool_id: receipt.shard_id.0,
            selections: vec![NodeSelection {
                node_id: receipt.creator.clone(),
                time_weight: 0.91,
                reputation: 0.97,
                shard_affinity: receipt.shard_id.0,
                longevity_hours: 24,
                proof_of_burn_tokens: 0.42,
                zkp_validations: 1_024,
            }],
        }],
        shard_utilization: vec![ShardLoad {
            shard_id: receipt.shard_id.0,
            throughput_tps: 52_000_000.0,
            tier_path: [
                TierDesignation::Tuplechain,
                TierDesignation::IcosupleCore,
                TierDesignation::QsDag,
            ],
            elected_leader: Some("chronosync-alpha".into()),
        }],
        dag_witness: DagWitness {
            nodes: vec![DagNode {
                node_id: format!("dag-node-{}", frame.sequence),
                parents: vec!["genesis".into()],
                shard_affinity: receipt.shard_id.0,
                leader: "chronosync-alpha".into(),
                payload_bytes: TIER_BYTES,
                transactions_carried: transactions,
            }],
        },
        rejected_transactions: 0,
    }
}

fn derive_two_qubit_plan(cursor: &mut EntropyCursor<'_>) -> TwoQubitPlan {
    const BASE_ALICE: [f64; 2] = [0.0, PI / 2.0];
    const BASE_BOB: [f64; 2] = [PI / 4.0, -PI / 4.0];

    TwoQubitPlan {
        alice: MeasurementPair {
            label: "alice".into(),
            angles: [
                BASE_ALICE[0] + cursor.jitter(0.05),
                BASE_ALICE[1] + cursor.jitter(0.05),
            ],
        },
        bob: MeasurementPair {
            label: "bob".into(),
            angles: [
                BASE_BOB[0] + cursor.jitter(0.05),
                BASE_BOB[1] + cursor.jitter(0.05),
            ],
        },
        classical_limit: 2.0,
        tsirelson_bound: 2.0 * 2.0_f64.sqrt(),
    }
}

fn derive_five_d_plan(
    cursor: &mut EntropyCursor<'_>,
    dims: usize,
) -> (Vec<FiveDAxis>, Vec<HyperedgePlan>) {
    let mut axes = Vec::with_capacity(dims);
    for dimension in 0..dims {
        let base_phi = 0.0;
        axes.push(FiveDAxis {
            dimension,
            primary: BlochAngles {
                theta: PI / 2.0 + cursor.jitter(0.04),
                phi: base_phi + cursor.jitter(0.12),
            },
            secondary: BlochAngles {
                theta: PI / 2.0 + cursor.jitter(0.04),
                phi: PI / 2.0 + cursor.jitter(0.12),
            },
        });
    }
    let hyperedges = derive_hyperedges(cursor, dims);
    (axes, hyperedges)
}

fn derive_hyperedges(_cursor: &mut EntropyCursor<'_>, dims: usize) -> Vec<HyperedgePlan> {
    const HYPEREDGE_COUNT: usize = 16;
    (0..HYPEREDGE_COUNT)
        .map(|idx| HyperedgePlan {
            label: format!("edge-{idx:02}"),
            participants: (0..dims).collect(),
            basis: vec!["primary".to_string(); dims],
            sign: 1.0,
        })
        .collect()
}

fn export_chsh_bridge(snapshot: &ChshBridgeSnapshot) -> PathBuf {
    let path = PathBuf::from("target/chsh_bridge_state.json");
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(snapshot).expect("serialize bridge");
    fs::write(&path, json).expect("write bridge");
    path
}

struct EntropyCursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> EntropyCursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn next_fraction(&mut self) -> f64 {
        self.next_u16() as f64 / u16::MAX as f64
    }

    fn jitter(&mut self, scale: f64) -> f64 {
        (self.next_fraction() - 0.5) * 2.0 * scale
    }

    fn next_u16(&mut self) -> u16 {
        if self.bytes.is_empty() {
            return 0;
        }
        if self.offset + 2 > self.bytes.len() {
            self.offset = 0;
        }
        let value = u16::from_le_bytes([
            self.bytes[self.offset],
            self.bytes[(self.offset + 1) % self.bytes.len()],
        ]);
        self.offset = (self.offset + 2) % self.bytes.len();
        value
    }
}

#[derive(Serialize)]
struct ChshBridgeSnapshot {
    qrng_epoch: u64,
    qrng_seed_hex: String,
    qrng_bits: usize,
    tuple_receipt: TupleReceiptDigest,
    hyper_tuple_hash: String,
    hyper_tuple_bits: usize,
    vertex: VertexReceiptDigest,
    two_qubit: TwoQubitPlan,
    five_d: FiveDPlan,
    hyperedges: Vec<HyperedgePlan>,
    dag_state: DagStateDigest,
}

#[derive(Serialize)]
struct TupleReceiptDigest {
    tuple_id: String,
    shard_id: u16,
    version: u32,
    expiry_ms: u64,
    creator: String,
}

impl From<&TupleReceipt> for TupleReceiptDigest {
    fn from(value: &TupleReceipt) -> Self {
        Self {
            tuple_id: value.tuple_id.to_string(),
            shard_id: value.shard_id.0,
            version: value.version,
            expiry_ms: value.expiry,
            creator: value.creator.clone(),
        }
    }
}

#[derive(Serialize)]
struct VertexReceiptDigest {
    vertex_id: String,
    tw_score: f64,
    storage: String,
    ann_similarity: f32,
    entanglement: f32,
    parents: usize,
}

impl From<&VertexReceipt> for VertexReceiptDigest {
    fn from(value: &VertexReceipt) -> Self {
        Self {
            vertex_id: value.vertex_id.to_string(),
            tw_score: value.tw_score,
            storage: format!("{:?}", value.storage),
            ann_similarity: value.ann_similarity,
            entanglement: value.entanglement_coefficient,
            parents: value.parents,
        }
    }
}

#[derive(Serialize)]
struct TwoQubitPlan {
    alice: MeasurementPair,
    bob: MeasurementPair,
    classical_limit: f64,
    tsirelson_bound: f64,
}

#[derive(Serialize)]
struct MeasurementPair {
    label: String,
    angles: [f64; 2],
}

#[derive(Serialize)]
struct FiveDPlan {
    axes: Vec<FiveDAxis>,
    dims: usize,
    target_violation: f64,
}

#[derive(Serialize)]
struct FiveDAxis {
    dimension: usize,
    primary: BlochAngles,
    secondary: BlochAngles,
}

#[derive(Serialize)]
struct BlochAngles {
    theta: f64,
    phi: f64,
}

#[derive(Serialize)]
struct HyperedgePlan {
    label: String,
    participants: Vec<usize>,
    basis: Vec<String>,
    sign: f64,
}

#[derive(Serialize)]
struct DagStateDigest {
    head_id: String,
    entries: Vec<(String, String)>,
}

impl From<&StateSnapshot> for DagStateDigest {
    fn from(snapshot: &StateSnapshot) -> Self {
        let entries = snapshot
            .values
            .iter()
            .take(4)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        Self {
            head_id: snapshot.head_id.clone(),
            entries,
        }
    }
}
