//! Core sentry loop that watches configured relayers and records telemetry.
//!
//! # Example
//! ```
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

use pqcnet_crypto::{CryptoError, CryptoProvider};
use pqcnet_networking::{NetworkClient, NetworkingError};
use pqcnet_telemetry::{TelemetryError, TelemetryHandle};
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

#[cfg(test)]
mod tests {
    use pqcnet_crypto::CryptoConfig;
    use pqcnet_networking::NetworkingConfig;
    use pqcnet_telemetry::TelemetryConfig;

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
        let cfg = config();
        let crypto = CryptoProvider::from_config(&cfg.crypto).unwrap();
        let network = NetworkClient::from_config(&cfg.crypto.node_id, cfg.networking.clone());
        let telemetry = TelemetryHandle::from_config(cfg.telemetry.clone());
        let mut service = SentryService::new(&cfg, crypto, network, telemetry.clone());
        let report = service.run_iteration(false).unwrap();
        assert_eq!(report.processed_watchers, 1);
        let snapshot = telemetry.flush();
        assert_eq!(snapshot.counters["sentry.success"], 1);
    }
}
