#![cfg(feature = "sim")]

use autheo_pqcnet_tuplechain::{
    ProofScheme, TupleChainConfig, TupleChainKeeper, TupleChainSim, TupleIntent,
};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut keeper = TupleChainKeeper::new(TupleChainConfig::default())
        .allow_creator("did:autheo:l1/kernel")
        .allow_creator("did:autheo:sensors/lidar");

    let intents = vec![
        TupleIntent::identity("did:autheo:alice", "autheoid-passport", 86_400_000)
            .with_object_json(json!({
                "credential": "autheoid-passport",
                "epoch": 42,
                "scope": ["age", "citizenship"]
            })),
        TupleIntent::identity("did:autheo:bob", "kyc-proof", 43_200_000)
            .with_creator("did:autheo:sensors/lidar")
            .with_predicate("attests")
            .with_proof(ProofScheme::Signature, "dilithium:tuple"),
        TupleIntent::identity("did:autheo:carol", "ai-intent", 21_600_000)
            .with_predicate("routes")
            .with_object_json(json!({
                "intent": "deploy-agent",
                "mesh_topic": "waku/qdag",
                "qos": "low-latency"
            })),
    ];

    let mut sim = TupleChainSim::new(1337);
    let report = sim.drive_epoch(&mut keeper, intents, 1_700_000_000_000);

    println!("TupleChain demo");
    println!("commit receipts: {}", report.receipts.len());
    for receipt in &report.receipts {
        println!(
            "{} -> {} v{} expires@{}",
            receipt.creator, receipt.tuple_id, receipt.version, receipt.expiry
        );
    }

    println!("\nShard utilization (first 6 entries):");
    for shard in report.shard_utilization.iter().take(6) {
        println!(
            "{} {:?}: tuples={} load={:.02}",
            shard.shard_id, shard.tier, shard.tuples, shard.load_factor
        );
    }

    if report.errors.is_empty() {
        println!("\nno keeper errors");
    } else {
        println!("\nerrors: {}", report.errors.len());
    }

    if report.expired.is_empty() {
        println!("no expiries in this epoch");
    } else {
        println!("expired handles: {}", report.expired.len());
    }

    Ok(())
}
