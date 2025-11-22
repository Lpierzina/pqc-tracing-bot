use autheo_pqcnet_5dqeh::{HypergraphModule, QehConfig, TemporalWeightModel};
use autheo_pqcnet_chronosync::{
    ChronosyncConfig, ChronosyncKeeper, ChronosyncKeeperReport, DagNode, DagWitness, EpochReport,
    ShardLoad, DEFAULT_TIER_PATH,
};
use autheo_pqcnet_icosuple::{HyperTupleBuilder, HyperTupleConfig, ICOSUPLE_BYTES};
use autheo_pqcnet_tuplechain::{
    ProofScheme, TupleChainConfig, TupleChainKeeper, TuplePayload, TupleReceipt,
};

#[test]
fn builder_generates_full_hyper_tuple() {
    let receipt = sample_tuple_receipt();
    let epoch_report = sample_epoch_report();
    let keeper_report = sample_keeper_report(&epoch_report);
    let builder = HyperTupleBuilder::default();
    let hyper_tuple = builder.assemble(&receipt, &epoch_report, &keeper_report);

    assert_eq!(hyper_tuple.total_bytes(), ICOSUPLE_BYTES);
    assert_eq!(hyper_tuple.metadata.tier_assignments.len(), 20);
    assert_eq!(
        hyper_tuple.metadata.extensions.len(),
        keeper_report.applied_vertices.len()
    );
    assert_eq!(hyper_tuple.pqc_envelope.qrng_entropy_bits, 512);
    assert_eq!(hyper_tuple.encode().len(), ICOSUPLE_BYTES);
}

#[test]
fn infinite_extensions_follow_chronosync_vertices() {
    let receipt = sample_tuple_receipt();
    let mut epoch_report = sample_epoch_report();
    epoch_report.dag_witness.nodes.push(DagNode {
        node_id: "node-extra".into(),
        parents: vec!["node-0".into()],
        shard_affinity: 2,
        leader: "did:autheo:gamma".into(),
        payload_bytes: 3_072,
        transactions_carried: 2_000,
    });
    let keeper_report = sample_keeper_report(&epoch_report);
    let config = HyperTupleConfig {
        vector_embedding_dims: 8_192,
        qrng_entropy_bits: 768,
    };
    let builder = HyperTupleBuilder::new(config);
    let hyper_tuple = builder.assemble(&receipt, &epoch_report, &keeper_report);

    assert!(
        hyper_tuple.metadata.extensions.len() >= 2,
        "expected extensions for each Chronosync vertex"
    );
    assert!(
        hyper_tuple.metadata.entanglement_coefficient > 0.8,
        "entanglement should scale with extensions"
    );
    assert_eq!(hyper_tuple.pqc_envelope.qrng_entropy_bits, 768);
}

fn sample_tuple_receipt() -> TupleReceipt {
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

fn sample_epoch_report() -> EpochReport {
    EpochReport {
        epoch_index: 7,
        aggregated_tps: 5_000_000.0,
        fairness_gini: 0.2,
        pools: Vec::new(),
        shard_utilization: vec![ShardLoad {
            shard_id: 0,
            throughput_tps: 2_500_000.0,
            tier_path: DEFAULT_TIER_PATH,
            elected_leader: Some("did:autheo:alpha".into()),
        }],
        dag_witness: DagWitness {
            nodes: vec![DagNode {
                node_id: "node-0".into(),
                parents: Vec::new(),
                shard_affinity: 0,
                leader: "did:autheo:alpha".into(),
                payload_bytes: 2_048,
                transactions_carried: 1_000,
            }],
        },
        rejected_transactions: 0,
    }
}

fn sample_keeper_report(epoch_report: &EpochReport) -> ChronosyncKeeperReport {
    let chrono_config = ChronosyncConfig::default();
    let module = HypergraphModule::new(QehConfig::default(), TemporalWeightModel::default());
    let mut keeper = ChronosyncKeeper::new(chrono_config, module);
    keeper
        .ingest_epoch_report(epoch_report)
        .expect("chronosync keeper report")
}
