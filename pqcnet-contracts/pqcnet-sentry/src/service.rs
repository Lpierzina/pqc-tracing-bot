//! Core sentry loop that watches configured relayers and records telemetry.
//!
//! # Example
//! ```no_run
//! use pqcnet_crypto::CryptoProvider;
//! use pqcnet_networking::NetworkClient;
//! use pqcnet_sentry::config::Config;
//! use pqcnet_sentry::service::SentryService;
//! use pqcnet_telemetry::TelemetryHandle;
//!
//! let cfg = Config::sample();
//! let crypto = CryptoProvider::from_config(&cfg.crypto).unwrap();
//! let network = NetworkClient::from_config(&cfg.crypto.node_id, cfg.networking.clone());
//! let telemetry = TelemetryHandle::from_config(cfg.telemetry.clone());
//! let mut service = SentryService::new(&cfg, crypto, network, telemetry);
//! let report = service.run_iteration(true).unwrap();
//! assert_eq!(report.processed_watchers, cfg.sentry.watchers.len());
//! ```

use pqcnet_crypto::{CryptoError, CryptoProvider, KemRationale};
use pqcnet_networking::{NetworkClient, NetworkingError};
use pqcnet_telemetry::{KemUsageReason, KemUsageRecord, TelemetryError, TelemetryHandle};
use thiserror::Error;

use crate::config::{Config, SentrySection};

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("network error for peer {peer}: {source}")]
    Network {
        peer: String,
        #[source]
        source: NetworkingError,
    },
    #[error(transparent)]
    Telemetry(#[from] TelemetryError),
    #[error(transparent)]
    Crypto(#[from] CryptoError),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SentryReport {
    pub processed_watchers: usize,
    pub quorum_threshold: u8,
}

pub struct SentryService {
    config: SentrySection,
    crypto: CryptoProvider,
    network: NetworkClient,
    telemetry: TelemetryHandle,
}

impl SentryService {
    pub fn new(
        config: &Config,
        crypto: CryptoProvider,
        network: NetworkClient,
        telemetry: TelemetryHandle,
    ) -> Self {
        Self {
            config: config.sentry.clone(),
            crypto,
            network,
            telemetry,
        }
    }

    pub fn run_iteration(&mut self, dry_run: bool) -> Result<SentryReport, ServiceError> {
        for watcher in &self.config.watchers {
            let derived = self.crypto.derive_shared_key(watcher)?;
            let kem_status = self.crypto.kem_status();
            self.telemetry.record_kem_event(KemUsageRecord {
                label: format!("sentry::{}", hex::encode(derived.key_id.0)),
                scheme: kem_status.scheme.as_str().into(),
                reason: map_kem_reason(kem_status.rationale),
                backup_only: kem_status.backup_only,
            });
            let payload = format!(
                "watcher-handshake:{}:{}:{}",
                watcher,
                hex::encode(derived.material),
                hex::encode(&derived.ciphertext)
            );
            if dry_run {
                self.telemetry.record_counter("sentry.dry_run", 1)?;
            } else {
                let receipt = self
                    .network
                    .publish(watcher, payload.into_bytes())
                    .map_err(|source| ServiceError::Network {
                        peer: watcher.clone(),
                        source,
                    })?;
                self.telemetry
                    .record_latency_ms("sentry.latency_ms", receipt.latency_ms);
                self.telemetry.record_counter("sentry.success", 1)?;
            }
        }

        Ok(SentryReport {
            processed_watchers: self.config.watchers.len(),
            quorum_threshold: self.config.quorum_threshold,
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

    fn config() -> Config {
        Config {
            sentry: SentrySection {
                watchers: vec!["peer-a".into()],
                poll_interval_ms: 100,
                quorum_threshold: 1,
            },
            crypto: CryptoConfig::sample("sentry"),
            networking: NetworkingConfig::sample("0.0.0.0:1"),
            telemetry: TelemetryConfig::sample("http://localhost"),
        }
    }

    #[test]
    fn run_iteration_increments_success_counter() {
        let mut cfg = config();
        let network_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        cfg.networking.peers = vec![PeerConfig {
            id: "peer-a".into(),
            address: network_listener.local_addr().unwrap().to_string(),
        }];
        let collector = HttpCollector::start(1);
        cfg.telemetry = TelemetryConfig::sample(&collector.url);
        let crypto = CryptoProvider::from_config(&cfg.crypto).unwrap();
        let network = NetworkClient::from_config(&cfg.crypto.node_id, cfg.networking.clone());
        let telemetry = TelemetryHandle::from_config(cfg.telemetry.clone());
        let mut service = SentryService::new(&cfg, crypto, network, telemetry.clone());
        let report = service.run_iteration(false).unwrap();
        assert_eq!(report.processed_watchers, 1);
        drop(network_listener);
        let snapshot = telemetry.flush().unwrap();
        assert_eq!(snapshot.counters["sentry.success"], 1);
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
                        let mut buf = [0u8; 2048];
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
