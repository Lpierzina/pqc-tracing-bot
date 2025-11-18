use pqcnet_telemetry::{TelemetryConfig, TelemetryHandle};

fn main() {
    let telemetry = TelemetryHandle::from_config(TelemetryConfig::sample("http://localhost:4318"));

    for _ in 0..3 {
        telemetry
            .record_counter("ingest.success", 1)
            .expect("within u64 range");
    }
    telemetry.record_latency_ms("pipeline", 42);
    telemetry.record_latency_ms("pipeline", 54);

    let snapshot = telemetry.flush();
    println!(
        "[pqcnet-telemetry] counters={:?} latencies={:?}",
        snapshot.counters, snapshot.latencies_ms
    );
}
