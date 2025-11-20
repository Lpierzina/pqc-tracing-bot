use autheo_pqcnet_tuplechain::{
    ProofScheme, TupleChainConfig, TupleChainKeeper, TupleChainSim, TupleIntent, TuplePayload,
    TupleStatus,
};
use pretty_assertions::assert_eq;
use serde_json::json;

fn demo_tuple(expiry: u64) -> TuplePayload {
    TuplePayload::builder("did:autheo:test", "owns")
        .object_value(json!({"asset": "demo"}))
        .proof(ProofScheme::Zkp, b"proof", "demo-zkp")
        .expiry(expiry)
        .build()
}

#[test]
fn version_history_is_accessible() {
    let mut keeper =
        TupleChainKeeper::new(TupleChainConfig::default()).allow_creator("did:autheo:l1/kernel");
    let ts = 1_700_000_000_000;
    let receipt_a = keeper
        .store_tuple("did:autheo:l1/kernel", demo_tuple(ts + 10_000), ts)
        .expect("first tuple");
    let receipt_b = keeper
        .store_tuple("did:autheo:l1/kernel", demo_tuple(ts + 20_000), ts + 500)
        .expect("second tuple");

    assert_eq!(receipt_a.tuple_id, receipt_b.tuple_id);
    assert_eq!(receipt_b.version, receipt_a.version + 1);

    let latest = keeper
        .ledger()
        .latest(&receipt_a.tuple_id)
        .expect("latest tuple");
    assert_eq!(latest.version, receipt_b.version);

    let historical = keeper
        .ledger()
        .by_version(&receipt_a.tuple_id, receipt_a.version)
        .expect("historical tuple");
    assert_eq!(historical.status, TupleStatus::Historical);
}

#[test]
fn prune_expired_entries_and_simulate_epoch() {
    let mut keeper =
        TupleChainKeeper::new(TupleChainConfig::default()).allow_creator("did:autheo:l1/kernel");
    keeper
        .store_tuple("did:autheo:l1/kernel", demo_tuple(1_700_000_000_100), 1_700_000_000_000)
        .unwrap();
    let expired = keeper.ledger_mut().prune_expired(1_700_000_001_000);
    assert_eq!(expired.len(), 1);

    let mut sim = TupleChainSim::new(99);
    let intents = vec![TupleIntent::identity("did:autheo:alice", "passport", 10_000)];
    let report = sim.drive_epoch(&mut keeper, intents, 1_700_000_005_000);
    assert!(report.errors.is_empty());
    assert_eq!(report.receipts.len(), 1);
    assert!(!report.shard_utilization.is_empty());
}
