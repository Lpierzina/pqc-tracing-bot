use autheo_pqc_core::qace::{
    GaQace, PathSet, QaceEngine, QaceGaConfig, QaceMetrics, QaceRequest, QaceWeights,
};
use autheo_pqc_core::qstp::{MeshPeerId, MeshQosClass, MeshRoutePlan, TunnelId};

struct Scenario {
    name: &'static str,
    epoch: u64,
    metrics: QaceMetrics,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tunnel_id = TunnelId([0xAB; 16]);
    let mut routes = build_routes();
    let primary = routes.remove(0);
    let mut engine = GaQace::new(
        QaceGaConfig {
            rng_seed: Some(9),
            ..Default::default()
        },
        QaceWeights::default(),
    );

    let scenarios = vec![
        Scenario {
            name: "steady-state",
            epoch: 11,
            metrics: QaceMetrics {
                latency_ms: 3,
                loss_bps: 1_100,
                threat_score: 5,
                route_changes: 0,
                ..Default::default()
            },
        },
        Scenario {
            name: "congested-mesh",
            epoch: 12,
            metrics: QaceMetrics {
                latency_ms: 15,
                loss_bps: 12_500,
                threat_score: 12,
                route_changes: 1,
                jitter_ms: 8,
                bandwidth_mbps: 40,
                ..Default::default()
            },
        },
        Scenario {
            name: "threat-injection",
            epoch: 13,
            metrics: QaceMetrics {
                latency_ms: 4,
                loss_bps: 2_400,
                threat_score: 92,
                route_changes: 2,
                chaos_level: 10,
                ..Default::default()
            },
        },
    ];

    println!("== QACE GA Simulation ==");
    println!(
        "{:<18} {:<12} {:<12} {:<12} {:<14}",
        "scenario", "action", "primary", "score", "confidence"
    );
    for scenario in scenarios {
        let decision = engine.evaluate(QaceRequest {
            tunnel_id: &tunnel_id,
            telemetry_epoch: scenario.epoch,
            metrics: scenario.metrics,
            path_set: PathSet::new(primary.clone(), routes.clone()),
        })?;
        let action = format!("{:?}", decision.action);
        println!(
            "{:<18} {:<12} {:<12} {:<12} {:<14.2}",
            scenario.name,
            action,
            decision.path_set.primary.topic,
            decision.score,
            decision.convergence.confidence,
        );
    }

    Ok(())
}

fn build_routes() -> Vec<MeshRoutePlan> {
    vec![
        MeshRoutePlan {
            topic: "waku/mesh/primary".into(),
            hops: vec![MeshPeerId::derive("hop/a"), MeshPeerId::derive("hop/b")],
            qos: MeshQosClass::LowLatency,
            epoch: 1,
        },
        MeshRoutePlan {
            topic: "waku/mesh/failsafe".into(),
            hops: vec![MeshPeerId::derive("hop/c")],
            qos: MeshQosClass::Control,
            epoch: 2,
        },
        MeshRoutePlan {
            topic: "waku/mesh/high-throughput".into(),
            hops: vec![
                MeshPeerId::derive("hop/d"),
                MeshPeerId::derive("hop/e"),
                MeshPeerId::derive("hop/f"),
            ],
            qos: MeshQosClass::Gossip,
            epoch: 3,
        },
        MeshRoutePlan {
            topic: "waku/mesh/fractal".into(),
            hops: vec![MeshPeerId::derive("hop/g")],
            qos: MeshQosClass::LowLatency,
            epoch: 4,
        },
    ]
}
