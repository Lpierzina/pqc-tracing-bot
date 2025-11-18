//! Lightweight telemetry facade for pqcnet binaries. The goal is to provide
//! structured counters/latencies without requiring external exporters so tests
//! can assert instrumentation behavior.

use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::SystemTime,
};
use thiserror::Error;

#[cfg(any(
    all(feature = "dev", feature = "test"),
    all(feature = "dev", feature = "prod"),
    all(feature = "test", feature = "prod")
))]
compile_error!(
    "Only one of the `dev`, `test`, or `prod` features may be enabled for pqcnet-telemetry."
);

#[cfg(feature = "dev")]
const DEFAULT_FLUSH_MS: u64 = 1_000;
#[cfg(feature = "test")]
const DEFAULT_FLUSH_MS: u64 = 500;
#[cfg(feature = "prod")]
const DEFAULT_FLUSH_MS: u64 = 5_000;

fn default_flush_interval_ms() -> u64 {
    DEFAULT_FLUSH_MS
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct TelemetryConfig {
    /// Endpoint where telemetry would be shipped (not used in mock impl).
    pub endpoint: String,
    /// Flush cadence in milliseconds.
    #[serde(default = "default_flush_interval_ms")]
    pub flush_interval_ms: u64,
    /// Global labels appended to every snapshot.
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
}

impl TelemetryConfig {
    pub fn sample(endpoint: &str) -> Self {
        Self {
            endpoint: endpoint.to_owned(),
            flush_interval_ms: default_flush_interval_ms(),
            labels: BTreeMap::from([("component".into(), "sentry".into())]),
        }
    }
}

#[derive(Debug, Error)]
pub enum TelemetryError {
    #[error("counter overflow for metric {0}")]
    CounterOverflow(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TelemetrySnapshot {
    pub timestamp: SystemTime,
    pub labels: BTreeMap<String, String>,
    pub counters: BTreeMap<String, u64>,
    pub latencies_ms: BTreeMap<String, Vec<u64>>,
}

#[derive(Default)]
struct TelemetryState {
    counters: BTreeMap<String, u64>,
    latencies_ms: BTreeMap<String, Vec<u64>>,
}

#[derive(Clone)]
pub struct TelemetryHandle {
    config: TelemetryConfig,
    state: Arc<Mutex<TelemetryState>>,
}

impl TelemetryHandle {
    pub fn from_config(config: TelemetryConfig) -> Self {
        Self {
            config,
            state: Arc::new(Mutex::new(TelemetryState::default())),
        }
    }

    pub fn record_counter(&self, name: &str, delta: u64) -> Result<(), TelemetryError> {
        let mut guard = self.state.lock().unwrap();
        let entry = guard.counters.entry(name.to_owned()).or_default();
        *entry = entry
            .checked_add(delta)
            .ok_or_else(|| TelemetryError::CounterOverflow(name.to_owned()))?;
        Ok(())
    }

    pub fn record_latency_ms(&self, name: &str, value: u64) {
        let mut guard = self.state.lock().unwrap();
        guard
            .latencies_ms
            .entry(name.to_owned())
            .or_default()
            .push(value);
    }

    pub fn flush(&self) -> TelemetrySnapshot {
        let mut guard = self.state.lock().unwrap();
        let snapshot = TelemetrySnapshot {
            timestamp: SystemTime::now(),
            labels: self.config.labels.clone(),
            counters: guard.counters.clone(),
            latencies_ms: guard.latencies_ms.clone(),
        };
        guard.counters.clear();
        guard.latencies_ms.clear();
        snapshot
    }

    pub fn flush_interval(&self) -> u64 {
        self.config.flush_interval_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handle() -> TelemetryHandle {
        TelemetryHandle::from_config(TelemetryConfig::sample("http://localhost:4318"))
    }

    #[test]
    fn records_counters_and_latencies() {
        let handle = handle();
        handle.record_counter("ingest.success", 1).unwrap();
        handle.record_counter("ingest.success", 2).unwrap();
        handle.record_latency_ms("pipeline", 42);
        let snapshot = handle.flush();
        assert_eq!(snapshot.counters["ingest.success"], 3);
        assert_eq!(snapshot.latencies_ms["pipeline"], vec![42]);
    }

    #[test]
    fn detects_counter_overflow() {
        let handle = handle();
        handle.record_counter("ingest.success", u64::MAX).unwrap();
        let err = handle.record_counter("ingest.success", 1).unwrap_err();
        assert!(matches!(err, TelemetryError::CounterOverflow(_)));
    }

    #[test]
    fn flush_clears_state() {
        let handle = handle();
        handle.record_counter("ingest.success", 1).unwrap();
        handle.flush();
        let second = handle.flush();
        assert!(second.counters.is_empty());
    }
}
