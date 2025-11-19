use pqcnet_qace::{PathSet, QaceEngine, QaceMetrics, QaceRequest, QaceRoute, SimpleQace};

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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let primary = DemoRoute::new("primary", 2, 4, 8);
    let alternate = DemoRoute::new("shielded", 1, 5, 7);
    let mut engine = SimpleQace::default();

    let threat_decision = engine.evaluate(QaceRequest {
        telemetry_epoch: 42,
        metrics: QaceMetrics {
            threat_score: 91,
            latency_ms: 3,
            ..Default::default()
        },
        path_set: PathSet::new(primary.clone(), vec![alternate.clone()]),
    })?;
    println!(
        "threat scenario => action={:?}, new_primary={}",
        threat_decision.action, threat_decision.path_set.primary.label
    );

    let rekey_decision = engine.evaluate(QaceRequest {
        telemetry_epoch: 43,
        metrics: QaceMetrics {
            loss_bps: 9_500,
            latency_ms: 5,
            ..Default::default()
        },
        path_set: PathSet::new(primary, vec![alternate]),
    })?;
    println!(
        "loss scenario => action={:?}, score={}",
        rekey_decision.action, rekey_decision.score
    );

    Ok(())
}
