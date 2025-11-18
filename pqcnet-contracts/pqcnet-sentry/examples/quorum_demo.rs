use pqcnet_crypto::CryptoProvider;
use pqcnet_networking::NetworkClient;
use pqcnet_sentry::config::Config;
use pqcnet_sentry::service::SentryService;
use pqcnet_telemetry::TelemetryHandle;

fn main() {
    let cfg = Config::sample();
    let crypto = CryptoProvider::from_config(&cfg.crypto).expect("valid crypto config");
    let network = NetworkClient::from_config(&cfg.crypto.node_id, cfg.networking.clone());
    let telemetry = TelemetryHandle::from_config(cfg.telemetry.clone());

    let mut service = SentryService::new(&cfg, crypto, network, telemetry.clone());
    let report = service.run_iteration(false).expect("network succeeds");
    let snapshot = telemetry.flush();

    println!(
        "[pqcnet-sentry] processed {} watchers (quorum >= {})",
        report.processed_watchers, report.quorum_threshold
    );
    println!(
        "[pqcnet-sentry] counters={:?} latencies={:?}",
        snapshot.counters, snapshot.latencies_ms
    );
}
