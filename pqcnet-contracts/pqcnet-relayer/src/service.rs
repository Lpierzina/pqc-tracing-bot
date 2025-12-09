//! Simplified relayer queue that batches PQC messages before broadcasting.
//!
//! # Example
//! ```no_run
//! use pqcnet_crypto::CryptoProvider;
//! use pqcnet_networking::NetworkClient;
//! use pqcnet_relayer::config::{Config, RelayerMode};
//! use pqcnet_relayer::service::RelayerService;
//! use pqcnet_telemetry::TelemetryHandle;
//!
//! let mut cfg = Config::sample();
//! cfg.relayer.mode = RelayerMode::Egress;
//! let crypto = CryptoProvider::from_config(&cfg.crypto).unwrap();
//! let network = NetworkClient::from_config(&cfg.crypto.node_id, cfg.networking.clone());
//! let telemetry = TelemetryHandle::from_config(cfg.telemetry.clone());
//! let mut service = RelayerService::new(&cfg, crypto, network, telemetry);
//! let report = service.relay_once().unwrap();
//! assert!(report.delivered > 0);
//! ```

use std::collections::VecDeque;

use pqcnet_crypto::{CryptoError, CryptoProvider, KemRationale};
use pqcnet_networking::{NetworkClient, NetworkingError};
use pqcnet_telemetry::{KemUsageReason, KemUsageRecord, TelemetryError, TelemetryHandle};
use thiserror::Error;

use crate::config::{Config, RelayerMode, RelayerSection};

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error(transparent)]
    Telemetry(#[from] TelemetryError),
    #[error(transparent)]
    Crypto(#[from] CryptoError),
    #[error(transparent)]
    Network(#[from] NetworkingError),
}

#[derive(Clone, Debug, PartialEq)]
pub struct RelayerReport {
    pub delivered: usize,
    pub buffered: usize,
    pub mode: RelayerMode,
}

pub struct RelayerService {
    config: RelayerSection,
    crypto: CryptoProvider,
    network: NetworkClient,
    telemetry: TelemetryHandle,
    queue: VecDeque<Vec<u8>>,
}

impl RelayerService {
    pub fn new(
        config: &Config,
        crypto: CryptoProvider,
        network: NetworkClient,
        telemetry: TelemetryHandle,
    ) -> Self {
        Self {
            config: config.relayer.clone(),
            crypto,
            network,
            telemetry,
            queue: VecDeque::with_capacity(config.relayer.max_queue_depth as usize),
        }
    }

    fn fill_queue(&mut self) -> Result<(), ServiceError> {
        while self.queue.len() < self.config.max_queue_depth as usize {
            let idx = self.queue.len();
            let derived = self.crypto.derive_shared_key(&format!("batch-{}", idx))?;
            let signature = self.crypto.sign(&derived.material)?;
            let kem_status = self.crypto.kem_status();
            self.telemetry.record_kem_event(KemUsageRecord {
                label: format!("relayer::{}", hex::encode(derived.key_id.0)),
                scheme: kem_status.scheme.as_str().into(),
                reason: map_kem_reason(kem_status.rationale),
                backup_only: kem_status.backup_only,
            });
            let payload = format!(
                "{}:{}:{}:{}",
                self.config.mode.as_str(),
                hex::encode(derived.material),
                hex::encode(signature.bytes),
                hex::encode(&derived.ciphertext)
            );
            self.queue.push_back(payload.into_bytes());
        }
        Ok(())
    }

    pub fn relay_once(&mut self) -> Result<RelayerReport, ServiceError> {
        self.fill_queue()?;
        let mut delivered = 0usize;
        for _ in 0..self.config.batch_size {
            if let Some(message) = self.queue.pop_front() {
                match self.config.mode {
                    RelayerMode::Ingest => {
                        self.telemetry.record_counter("relayer.ingest", 1)?;
                    }
                    RelayerMode::Egress | RelayerMode::Bidirectional => {
                        let receipts = self.network.broadcast(&message)?;
                        delivered += receipts.len();
                        self.telemetry
                            .record_counter("relayer.egress", receipts.len() as u64)?;
                        for receipt in receipts {
                            self.telemetry
                                .record_latency_ms("relayer.latency_ms", receipt.latency_ms);
                        }
                        if matches!(self.config.mode, RelayerMode::Bidirectional) {
                            self.queue.push_back(message);
                        }
                    }
                }
            }
        }

        self.telemetry
            .record_latency_ms("relayer.retry_backoff_ms", self.config.retry_backoff_ms);

        Ok(RelayerReport {
            delivered,
            buffered: self.queue.len(),
            mode: self.config.mode,
        })
    }
}

fn map_kem_reason(rationale: KemRationale) -> KemUsageReason {
    match rationale {
        KemRationale::Normal => KemUsageReason::Normal,
        KemRationale::Drill => KemUsageReason::Drill,
        KemRationale::Fallback => KemUsageReason::Fallback,
    }
}

#[cfg(test)]
mod tests {
    use pqcnet_crypto::CryptoConfig;
    use pqcnet_networking::{NetworkingConfig, PeerConfig};
    use pqcnet_telemetry::TelemetryConfig;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    use super::*;

    fn config(mode: RelayerMode) -> Config {
        Config {
            relayer: RelayerSection {
                batch_size: 2,
                max_queue_depth: 16,
                retry_backoff_ms: 10,
                mode,
            },
            crypto: CryptoConfig::sample("relayer"),
            networking: NetworkingConfig::sample("0.0.0.0:1"),
            telemetry: TelemetryConfig::sample("http://localhost"),
        }
    }

    #[test]
    fn relay_once_records_metrics() {
        let mut cfg = config(RelayerMode::Egress);
        let mut listeners = Vec::new();
        cfg.networking.peers = ["peer-a", "peer-b"]
            .iter()
            .map(|id| {
                let listener = TcpListener::bind("127.0.0.1:0").unwrap();
                let addr = listener.local_addr().unwrap().to_string();
                listeners.push(listener);
                PeerConfig {
                    id: id.to_string(),
                    address: addr,
                }
            })
            .collect();
        let collector = HttpCollector::start(1);
        cfg.telemetry = TelemetryConfig::sample(&collector.url);
        let crypto = CryptoProvider::from_config(&cfg.crypto).unwrap();
        let network = NetworkClient::from_config(&cfg.crypto.node_id, cfg.networking.clone());
        let telemetry = TelemetryHandle::from_config(cfg.telemetry.clone());
        let mut service = RelayerService::new(&cfg, crypto, network, telemetry.clone());

        let report = service.relay_once().unwrap();
        assert!(report.delivered > 0);
        drop(listeners);
        let snapshot = telemetry.flush().unwrap();
        assert_eq!(snapshot.counters["relayer.egress"], report.delivered as u64);
        assert!(
            snapshot.kem_events.iter().any(|event| event.scheme == "kyber"),
            "expected kyber kem event"
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
}
