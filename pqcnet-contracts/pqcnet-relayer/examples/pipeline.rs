use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};

use pqcnet_crypto::CryptoProvider;
use pqcnet_networking::NetworkClient;
use pqcnet_relayer::config::{Config, RelayerMode};
use pqcnet_relayer::service::RelayerService;
use pqcnet_telemetry::TelemetryHandle;

fn main() {
    let mut cfg = Config::sample();
    cfg.relayer.mode = RelayerMode::Bidirectional;
    cfg.relayer.batch_size = 4;
    let listeners: Vec<TcpListener> = cfg
        .networking
        .peers
        .iter_mut()
        .map(|peer| {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind peer socket");
            peer.address = listener.local_addr().unwrap().to_string();
            listener
        })
        .collect();

    let collector = HttpCollector::start(1);
    cfg.telemetry = pqcnet_telemetry::TelemetryConfig::sample(&collector.url);
    let crypto = CryptoProvider::from_config(&cfg.crypto).expect("valid crypto config");
    let network = NetworkClient::from_config(&cfg.crypto.node_id, cfg.networking.clone());
    let telemetry = TelemetryHandle::from_config(cfg.telemetry.clone());

    let mut service = RelayerService::new(&cfg, crypto, network, telemetry.clone());
    let report = service.relay_once().expect("telemetry succeeds");
    drop(listeners);
    let snapshot = telemetry.flush().expect("telemetry export succeeds");

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

struct HttpCollector {
    url: String,
    join: Option<thread::JoinHandle<()>>,
}

impl HttpCollector {
    fn start(expected_requests: usize) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind collector");
        let addr = listener.local_addr().expect("collector addr");
        let handle = thread::spawn(move || {
            for _ in 0..expected_requests {
                if let Ok((mut stream, _)) = listener.accept() {
                    let mut buf = [0u8; 4096];
                    let _ = stream.read(&mut buf);
                    let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
                }
            }
        });
        Self {
            url: format!("http://{}", addr),
            join: Some(handle),
        }
    }
}

impl Drop for HttpCollector {
    fn drop(&mut self) {
        if let Some(handle) = self.join.take() {
            let _ = handle.join();
        }
    }
}
