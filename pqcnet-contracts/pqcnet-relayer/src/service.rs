use std::collections::VecDeque;

use pqcnet_crypto::CryptoProvider;
use pqcnet_networking::NetworkClient;
use pqcnet_telemetry::{TelemetryError, TelemetryHandle};
use thiserror::Error;

use crate::config::{Config, RelayerMode, RelayerSection};

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error(transparent)]
    Telemetry(#[from] TelemetryError),
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

    fn fill_queue(&mut self) {
        while self.queue.len() < self.config.max_queue_depth as usize {
            let idx = self.queue.len();
            let derived = self.crypto.derive_shared_key(&format!("batch-{}", idx));
            let signature = self.crypto.sign(&derived.material);
            let payload = format!(
                "{}:{}:{}",
                self.config.mode.as_str(),
                hex::encode(derived.material),
                hex::encode(signature.digest)
            );
            self.queue.push_back(payload.into_bytes());
        }
    }

    pub fn relay_once(&mut self) -> Result<RelayerReport, ServiceError> {
        self.fill_queue();
        let mut delivered = 0usize;
        for _ in 0..self.config.batch_size {
            if let Some(message) = self.queue.pop_front() {
                match self.config.mode {
                    RelayerMode::Ingest => {
                        self.telemetry.record_counter("relayer.ingest", 1)?;
                    }
                    RelayerMode::Egress | RelayerMode::Bidirectional => {
                        let receipts = self.network.broadcast(&message);
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

#[cfg(test)]
mod tests {
    use pqcnet_crypto::CryptoConfig;
    use pqcnet_networking::NetworkingConfig;
    use pqcnet_telemetry::TelemetryConfig;

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
        let cfg = config(RelayerMode::Egress);
        let crypto = CryptoProvider::from_config(&cfg.crypto).unwrap();
        let network = NetworkClient::from_config(&cfg.crypto.node_id, cfg.networking.clone());
        let telemetry = TelemetryHandle::from_config(cfg.telemetry.clone());
        let mut service = RelayerService::new(&cfg, crypto, network, telemetry.clone());

        let report = service.relay_once().unwrap();
        assert!(report.delivered > 0);
        let snapshot = telemetry.flush();
        assert_eq!(snapshot.counters["relayer.egress"], report.delivered as u64);
    }
}
