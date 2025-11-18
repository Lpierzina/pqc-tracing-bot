use pqcnet_crypto::CryptoProvider;
use pqcnet_networking::NetworkClient;
use pqcnet_relayer::config::{Config, RelayerMode};
use pqcnet_relayer::service::RelayerService;
use pqcnet_telemetry::TelemetryHandle;

fn main() {
    let mut cfg = Config::sample();
    cfg.relayer.mode = RelayerMode::Bidirectional;
    cfg.relayer.batch_size = 4;

    let crypto = CryptoProvider::from_config(&cfg.crypto).expect("valid crypto config");
    let network = NetworkClient::from_config(&cfg.crypto.node_id, cfg.networking.clone());
    let telemetry = TelemetryHandle::from_config(cfg.telemetry.clone());

    let mut service = RelayerService::new(&cfg, crypto, network, telemetry.clone());
    let report = service.relay_once().expect("telemetry succeeds");
    let snapshot = telemetry.flush();

    println!(
        "[pqcnet-relayer] mode={} delivered={} buffered={}",
        report.mode.as_str(),
        report.delivered,
        report.buffered
    );
    println!(
        "[pqcnet-relayer] counters={:?} latencies={:?}",
        snapshot.counters, snapshot.latencies_ms
    );
}
