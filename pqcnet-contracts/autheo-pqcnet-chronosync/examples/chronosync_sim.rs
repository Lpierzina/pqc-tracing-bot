#![cfg(feature = "sim")]

use autheo_pqcnet_chronosync::{ChronosyncConfig, ChronosyncNodeProfile, ChronosyncSim};

fn main() {
    let config = ChronosyncConfig::default();
    let nodes = vec![
        ChronosyncNodeProfile::new("did:autheo:alpha")
            .with_longevity_hours(24 * 540)
            .with_proof_of_burn(1.0)
            .with_zkp_validations(1_800),
        ChronosyncNodeProfile::new("did:autheo:beta")
            .with_longevity_hours(24 * 365)
            .with_proof_of_burn(0.7)
            .with_zkp_validations(900),
        ChronosyncNodeProfile::new("did:autheo:gamma")
            .with_longevity_hours(24 * 180)
            .with_proof_of_burn(0.4)
            .with_zkp_validations(1_200),
        ChronosyncNodeProfile::new("did:autheo:delta")
            .with_longevity_hours(24 * 90)
            .with_proof_of_burn(0.2)
            .with_zkp_validations(600),
        ChronosyncNodeProfile::new("did:autheo:epsilon")
            .with_longevity_hours(24 * 60)
            .with_proof_of_burn(0.9)
            .with_zkp_validations(2_200),
        ChronosyncNodeProfile::new("did:autheo:zeta")
            .with_longevity_hours(24 * 45)
            .with_proof_of_burn(0.3)
            .with_zkp_validations(420),
        ChronosyncNodeProfile::new("did:autheo:eta")
            .with_longevity_hours(24 * 30)
            .with_proof_of_burn(0.1)
            .with_zkp_validations(150)
            .with_suspicion_events(2),
        ChronosyncNodeProfile::new("did:autheo:theta")
            .with_longevity_hours(24 * 15)
            .with_proof_of_burn(0.05)
            .with_zkp_validations(75)
            .with_suspicion_events(4),
    ];

    let mut sim = ChronosyncSim::with_seed(1337, config);
    let report = sim.drive_epoch(&nodes, 1_500_000_000);

    println!(
        "chronosync epoch={} pools={} fairness_gini={:.3} rejected_txs={}",
        report.epoch_index,
        report.pools.len(),
        report.fairness_gini,
        report.rejected_transactions
    );

    for pool in &report.pools {
        let members: Vec<_> = pool
            .selections
            .iter()
            .map(|sel| format!("{} (TW={:.3})", sel.node_id, sel.time_weight))
            .collect();
        println!("  pool#{:02}: {}", pool.pool_id, members.join(", "));
    }

    println!(
        "\nfirst 5 shard loads (of {} total):",
        report.shard_utilization.len()
    );
    for shard in report.shard_utilization.iter().take(5) {
        println!(
            "  shard-{:04} => {:.2} TPS leader={}",
            shard.shard_id,
            shard.throughput_tps,
            shard
                .elected_leader
                .as_deref()
                .unwrap_or("(no leader in pool sample)")
        );
    }

    println!("\nQS-DAG witness:");
    for node in &report.dag_witness.nodes {
        println!(
            "  {} led by {} parents={:?} txs={} payload={}B",
            node.node_id, node.leader, node.parents, node.transactions_carried, node.payload_bytes
        );
    }
}
