use hex::FromHex;
use serde::Deserialize;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum QrngFeedError {
    #[error("failed to read QRNG feed: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse QRNG feed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("qrng_seed_hex must contain at least 64 hex chars (got {0})")]
    SeedTooShort(String),
    #[error("qrng_seed_hex contains invalid hex: {0}")]
    InvalidSeed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QrngFeedSource {
    Sandbox,
    Hardware { label: String },
}

impl Default for QrngFeedSource {
    fn default() -> Self {
        Self::Sandbox
    }
}

impl QrngFeedSource {
    pub fn label(&self) -> &str {
        match self {
            QrngFeedSource::Sandbox => "sandbox",
            QrngFeedSource::Hardware { label } => label.as_str(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct QrngFeedConfig {
    pub bridge_path: PathBuf,
    pub results_path: Option<PathBuf>,
    pub source: QrngFeedSource,
}

impl Default for QrngFeedConfig {
    fn default() -> Self {
        Self {
            bridge_path: PathBuf::from("pqcnet-contracts/target/chsh_bridge_state.json"),
            results_path: Some(PathBuf::from("pqcnet-contracts/target/chsh_results.json")),
            source: QrngFeedSource::Sandbox,
        }
    }
}

pub struct QrngFeed {
    config: QrngFeedConfig,
}

impl QrngFeed {
    pub fn new(config: QrngFeedConfig) -> Self {
        Self { config }
    }

    pub fn sample(&self) -> Result<QrngFeedSample, QrngFeedError> {
        let bridge_bytes = fs::read(&self.config.bridge_path)?;
        let snapshot: BridgeSnapshot = serde_json::from_slice(&bridge_bytes)?;
        let trimmed = snapshot.qrng_seed_hex.trim();
        if trimmed.len() < 64 {
            return Err(QrngFeedError::SeedTooShort(trimmed.to_owned()));
        }
        let seed_slice = &trimmed[..64];
        let seed = <[u8; 32]>::from_hex(seed_slice)
            .map_err(|_| QrngFeedError::InvalidSeed(trimmed.into()))?;
        let mut sample = QrngFeedSample {
            source: self.config.source.clone(),
            epoch: snapshot.qrng_epoch,
            seed_hex: seed_slice.to_owned(),
            seed,
            tuple_id: snapshot.tuple_receipt.tuple_id,
            shard_id: snapshot.tuple_receipt.shard_id,
            qrng_bits: snapshot.qrng_bits,
            hyper_tuple_bits: snapshot.hyper_tuple_bits,
            recorded_at: SystemTime::now(),
            chsh_summary: None,
            bridge_path: self.config.bridge_path.clone(),
            results_path: self.config.results_path.clone(),
        };
        if let Some(path) = &self.config.results_path {
            if let Ok(bytes) = fs::read(path) {
                if let Ok(results) = serde_json::from_slice::<ChshResultsFile>(&bytes) {
                    if let Some(summary) = results.summary() {
                        sample.chsh_summary = Some(summary);
                    }
                }
            }
        }
        Ok(sample)
    }
}

#[derive(Clone, Debug)]
pub struct QrngFeedSample {
    pub source: QrngFeedSource,
    pub epoch: u64,
    pub seed_hex: String,
    seed: [u8; 32],
    pub tuple_id: String,
    pub shard_id: u16,
    pub qrng_bits: usize,
    pub hyper_tuple_bits: usize,
    pub recorded_at: SystemTime,
    pub chsh_summary: Option<ChshResultSummary>,
    pub bridge_path: PathBuf,
    pub results_path: Option<PathBuf>,
}

impl QrngFeedSample {
    pub fn seed(&self) -> [u8; 32] {
        self.seed
    }

    pub fn recorded_unix_ms(&self) -> u128 {
        self.recorded_at
            .duration_since(UNIX_EPOCH)
            .map(|dur| dur.as_millis())
            .unwrap_or_default()
    }
}

#[derive(Clone, Debug)]
pub struct ChshResultSummary {
    pub two_qubit_exact: f64,
    pub two_qubit_sampled: f64,
    pub five_d_exact: f64,
    pub five_d_sampled: f64,
    pub shots: u32,
    pub depolarizing: f32,
}

#[derive(Deserialize)]
struct BridgeSnapshot {
    qrng_epoch: u64,
    qrng_seed_hex: String,
    qrng_bits: usize,
    hyper_tuple_bits: usize,
    #[serde(default)]
    tuple_receipt: BridgeTupleReceipt,
}

#[derive(Clone, Debug, Deserialize)]
struct BridgeTupleReceipt {
    tuple_id: String,
    shard_id: u16,
}

impl Default for BridgeTupleReceipt {
    fn default() -> Self {
        Self {
            tuple_id: "tuple/unknown".into(),
            shard_id: 0,
        }
    }
}

#[derive(Deserialize, Default)]
struct ChshResultsFile {
    #[serde(default)]
    two_qubit: Option<ResultSection>,
    #[serde(default)]
    five_d: Option<ResultSection>,
    #[serde(default)]
    shots: Option<u32>,
    #[serde(default)]
    depolarizing: Option<f32>,
}

impl ChshResultsFile {
    fn summary(&self) -> Option<ChshResultSummary> {
        let two = self.two_qubit.as_ref()?;
        let five = self.five_d.as_ref()?;
        Some(ChshResultSummary {
            two_qubit_exact: two.exact,
            two_qubit_sampled: two.sampled,
            five_d_exact: five.exact,
            five_d_sampled: five.sampled,
            shots: self.shots.unwrap_or_default(),
            depolarizing: self.depolarizing.unwrap_or_default(),
        })
    }
}

#[derive(Deserialize)]
struct ResultSection {
    exact: f64,
    sampled: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn loads_bridge_and_results() {
        let dir = tempdir().unwrap();
        let bridge_path = dir.path().join("bridge.json");
        let results_path = dir.path().join("results.json");

        let bridge = r#"{
            "qrng_epoch": 6,
            "qrng_seed_hex": "57a04bc3b237312bf6220f2fb2ed768ee03e1e5387dd06dc00dee6e65e39d594",
            "qrng_bits": 3072,
            "hyper_tuple_bits": 4096,
            "tuple_receipt": {"tuple_id": "tuple/fn-alpha", "shard_id": 42}
        }"#;
        fs::write(&bridge_path, bridge).unwrap();

        let results = r#"{
            "shots": 4096,
            "depolarizing": 0.05,
            "two_qubit": { "exact": 2.68, "sampled": 2.64 },
            "five_d": { "exact": 15.18, "sampled": 15.27 }
        }"#;
        let mut file = fs::File::create(&results_path).unwrap();
        file.write_all(results.as_bytes()).unwrap();

        let feed = QrngFeed::new(QrngFeedConfig {
            bridge_path: bridge_path.clone(),
            results_path: Some(results_path.clone()),
            source: QrngFeedSource::Sandbox,
        });

        let sample = feed.sample().expect("feed sample");
        assert_eq!(sample.epoch, 6);
        assert_eq!(sample.tuple_id, "tuple/fn-alpha");
        assert_eq!(sample.seed_hex.len(), 64);
        assert_eq!(sample.shard_id, 42);
        let summary = sample.chsh_summary.expect("summary present");
        assert_eq!(summary.shots, 4096);
        assert!((summary.two_qubit_exact - 2.68).abs() < 1e-6);
    }
}
