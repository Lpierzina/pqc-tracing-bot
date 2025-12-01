//! Production telemetry facade for pqcnet binaries. It records structured
//! counters/latencies and flushes them over OTLP/HTTP so relayers, sentries, and
//! standalone PQCNet nodes expose real metrics instead of simulations.
//!
//! # Quickstart
//! ```no_run
//! use pqcnet_telemetry::{TelemetryConfig, TelemetryHandle};
//!
//! let handle = TelemetryHandle::from_config(TelemetryConfig::sample("http://localhost:4318"));
//! handle.record_counter("ingest.success", 1).unwrap();
//! handle.record_latency_ms("pipeline", 42);
//! let snapshot = handle.flush().unwrap();
//! assert_eq!(snapshot.counters["ingest.success"], 1);
//! assert_eq!(snapshot.latencies_ms["pipeline"], vec![42]);
//! ```

pub mod abw34;

use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::SystemTime,
};
#[cfg(feature = "prod")]
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
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
    #[error("export failed: {0}")]
    Export(String),
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

    pub fn flush(&self) -> Result<TelemetrySnapshot, TelemetryError> {
        let mut guard = self.state.lock().unwrap();
        let snapshot = TelemetrySnapshot {
            timestamp: SystemTime::now(),
            labels: self.config.labels.clone(),
            counters: guard.counters.clone(),
            latencies_ms: guard.latencies_ms.clone(),
        };

        #[cfg(feature = "prod")]
        export_snapshot(&self.config, &snapshot)?;

        guard.counters.clear();
        guard.latencies_ms.clear();
        Ok(snapshot)
    }

    pub fn flush_interval(&self) -> u64 {
        self.config.flush_interval_ms
    }
}

#[cfg(feature = "prod")]
fn export_snapshot(
    config: &TelemetryConfig,
    snapshot: &TelemetrySnapshot,
) -> Result<(), TelemetryError> {
    #[derive(Serialize)]
    struct ExportPayload<'a> {
        timestamp_ms: u128,
        endpoint: &'a str,
        labels: &'a BTreeMap<String, String>,
        counters: &'a BTreeMap<String, u64>,
        latencies_ms: &'a BTreeMap<String, Vec<u64>>,
    }

    fn to_ms(ts: SystemTime) -> u128 {
        ts.duration_since(SystemTime::UNIX_EPOCH)
            .map(|dur| dur.as_millis())
            .unwrap_or_default()
    }

    let payload = ExportPayload {
        timestamp_ms: to_ms(snapshot.timestamp),
        endpoint: &config.endpoint,
        labels: &snapshot.labels,
        counters: &snapshot.counters,
        latencies_ms: &snapshot.latencies_ms,
    };

    let body =
        serde_json::to_string(&payload).map_err(|err| TelemetryError::Export(err.to_string()))?;

    let (addr, path) = parse_http_endpoint(&config.endpoint)?;
    let mut stream =
        TcpStream::connect(&addr).map_err(|err| TelemetryError::Export(err.to_string()))?;
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n{body}",
        host = host_header(&addr),
        len = body.len()
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|err| TelemetryError::Export(err.to_string()))?;
    stream
        .flush()
        .map_err(|err| TelemetryError::Export(err.to_string()))?;
    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader
        .read_line(&mut status_line)
        .map_err(|err| TelemetryError::Export(err.to_string()))?;
    if !(status_line.starts_with("HTTP/1.1 200") || status_line.starts_with("HTTP/1.0 200")) {
        return Err(TelemetryError::Export(format!(
            "collector responded with {status_line}"
        )));
    }
    let mut drain = Vec::new();
    let _ = reader.read_to_end(&mut drain);
    Ok(())
}

#[cfg(feature = "prod")]
fn parse_http_endpoint(endpoint: &str) -> Result<(String, String), TelemetryError> {
    const PREFIX: &str = "http://";
    if !endpoint.starts_with(PREFIX) {
        return Err(TelemetryError::Export(format!(
            "only http:// endpoints are supported (got {endpoint})"
        )));
    }
    let remainder = &endpoint[PREFIX.len()..];
    let mut parts = remainder.splitn(2, '/');
    let host = parts.next().unwrap_or_default();
    let path = format!("/{}", parts.next().unwrap_or(""));
    let addr = if host.contains(':') {
        host.to_string()
    } else {
        format!("{host}:80")
    };
    Ok((addr, path))
}

#[cfg(feature = "prod")]
fn host_header(addr: &str) -> &str {
    addr.rsplit_once(':').map(|(h, _)| h).unwrap_or(addr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

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

        fn config(&self) -> TelemetryConfig {
            TelemetryConfig::sample(&self.url)
        }
    }

    impl Drop for HttpCollector {
        fn drop(&mut self) {
            if let Some(handle) = self.join.take() {
                let _ = handle.join();
            }
        }
    }

    #[test]
    fn records_counters_and_latencies() {
        let collector = HttpCollector::start(1);
        let handle = TelemetryHandle::from_config(collector.config());
        handle.record_counter("ingest.success", 1).unwrap();
        handle.record_counter("ingest.success", 2).unwrap();
        handle.record_latency_ms("pipeline", 42);
        let snapshot = handle.flush().unwrap();
        assert_eq!(snapshot.counters["ingest.success"], 3);
        assert_eq!(snapshot.latencies_ms["pipeline"], vec![42]);
    }

    #[test]
    fn detects_counter_overflow() {
        let handle = TelemetryHandle::from_config(TelemetryConfig::sample("http://localhost:4318"));
        handle.record_counter("ingest.success", u64::MAX).unwrap();
        let err = handle.record_counter("ingest.success", 1).unwrap_err();
        assert!(matches!(err, TelemetryError::CounterOverflow(_)));
    }

    #[test]
    fn flush_clears_state() {
        let collector = HttpCollector::start(2);
        let handle = TelemetryHandle::from_config(collector.config());
        handle.record_counter("ingest.success", 1).unwrap();
        handle.flush().unwrap();
        let second = handle.flush().unwrap();
        assert!(second.counters.is_empty());
    }
}
