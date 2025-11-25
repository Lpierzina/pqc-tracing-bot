use pqcnet_qace::{
    GaQace, PathSet, QaceEngine, QaceGaConfig, QaceMetrics, QaceRequest, QaceRoute, QaceWeights,
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct DemoRoute {
    label: &'static str,
    hop_count: u32,
    qos_bias: i64,
    freshness: i64,
}

impl DemoRoute {
    fn new(label: &'static str, hop_count: u32, qos_bias: i64, freshness: i64) -> Self {
        Self {
            label,
            hop_count,
            qos_bias,
            freshness,
        }
    }
}

impl QaceRoute for DemoRoute {
    fn hop_count(&self) -> u32 {
        self.hop_count
    }

    fn qos_bias(&self) -> i64 {
        self.qos_bias
    }

    fn freshness(&self) -> i64 {
        self.freshness
    }

    fn is_viable(&self) -> bool {
        !self.label.is_empty()
    }
}

struct Scenario {
    name: &'static str,
    epoch: u64,
    metrics: QaceMetrics,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    println!("== QACE GA Failover Runbook ==");
    println!(
        "{:<18} {:<12} {:<12} {:<12} {:<14}",
        "scenario", "action", "primary", "score", "confidence"
    );
    for scenario in scenarios {
        let decision = engine.evaluate(QaceRequest {
            telemetry_epoch: scenario.epoch,
            metrics: scenario.metrics,
            path_set: PathSet::new(primary.clone(), routes.clone()),
        })?;
        let action = format!("{:?}", decision.action);
        println!(
            "{:<18} {:<12} {:<12} {:<12} {:<14.2}",
            scenario.name,
            action,
            decision.path_set.primary.label,
            decision.score,
            decision.convergence.confidence,
        );
    }

    Ok(())
}

fn build_routes() -> Vec<DemoRoute> {
    vec![
        DemoRoute::new("primary", 2, 5, 8),
        DemoRoute::new("failsafe", 1, 3, 6),
        DemoRoute::new("high-throughput", 3, 1, 5),
        DemoRoute::new("fractal", 1, 5, 9),
    ]
}
