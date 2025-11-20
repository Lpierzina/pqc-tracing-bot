use autheo_pqcnet_icosuple::{IcosupleNetworkConfig, IcosupleNetworkSim, TupleIntent};
use serde_json::json;

fn main() {
    let config = IcosupleNetworkConfig::default().with_assignments_per_icosuple(6);
    let intents = vec![
        TupleIntent::identity("did:autheo:alice", "autheoid-passport", 86_400_000)
            .with_estimated_tps(5_000_000.0),
        TupleIntent::new("did:autheo:bob", "depin-energy", 1_536).with_estimated_tps(8_500_000.0),
        TupleIntent::new("did:autheo:carol", "metaverse-avatar", 4_096)
            .with_estimated_tps(3_000_000.0),
    ];

    let mut sim = IcosupleNetworkSim::with_seed(1_337, config);
    let telemetry = sim.propagate_batch(&intents);

    println!(
        "Epoch {} · Aggregated {:.2} TPS · QS-DAG edges {} · Dynamic tiers {}",
        telemetry.epoch,
        telemetry.aggregated_tps,
        telemetry.qs_dag_edges,
        telemetry.dynamic_extensions
    );
    println!("--- Tier saturation snapshot (first 8 tiers) ---");
    for tier in telemetry.tiers.iter().take(8) {
        println!(
            "{} | shards={} | throughput={:.2} TPS | saturation={:.2}",
            tier.specialization.label(),
            tier.shards,
            tier.throughput_tps,
            tier.saturation
        );
    }

    let sample = telemetry.icosuples.get(0).expect("at least one icosuple");
    println!("--- Sample Icosuple ---");
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "id": sample.id,
            "subject": sample.subject,
            "total_bytes": sample.total_bytes(),
            "tier_assignments": sample.metadata.tier_assignments.iter().map(|assign| {
                json!({
                    "tier": assign.specialization.label(),
                    "shard": assign.shard_id,
                    "throughput_tps": assign.throughput_tps
                })
            }).collect::<Vec<_>>()
        }))
        .expect("json")
    );
}
