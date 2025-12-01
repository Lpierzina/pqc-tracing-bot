use serde::Serialize;
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::Path,
    sync::Mutex,
};

/// Structured ABW34 log entry capturing QRNG provenance + runtime load.
#[derive(Clone, Debug, Serialize)]
pub struct Abw34Record {
    pub timestamp_ms: u128,
    pub qrng_source: String,
    pub qrng_epoch: u64,
    pub qrng_tuple_id: String,
    pub qrng_seed_hex: String,
    pub qrng_bits: u64,
    pub qrng_shard_id: u16,
    pub shard_count: u16,
    pub noise_ratio: f32,
    pub qace_reroutes: u32,
    pub observed_tps_per_shard: f64,
    pub observed_tps_global: f64,
    pub kem_key_id: String,
    pub signing_key_id: String,
    pub kem_created_at: u64,
    pub kem_expires_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chsh_two_qubit_exact: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chsh_two_qubit_sampled: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chsh_five_d_exact: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chsh_five_d_sampled: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chsh_shots: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chsh_depolarizing: Option<f32>,
}

/// Thread-safe JSONL writer for ABW34 telemetry streams.
pub struct Abw34Logger {
    writer: Mutex<Box<dyn Write + Send>>,
}

impl Abw34Logger {
    /// Build a logger that appends JSONL entries to `path`, creating directories as needed.
    pub fn to_path(path: impl AsRef<Path>) -> io::Result<Self> {
        let path_ref = path.as_ref();
        if let Some(parent) = path_ref.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path_ref)?;
        Ok(Self {
            writer: Mutex::new(Box::new(file)),
        })
    }

    /// Wrap any writer (useful for tests).
    pub fn from_writer(writer: Box<dyn Write + Send>) -> Self {
        Self {
            writer: Mutex::new(writer),
        }
    }

    /// Append a record as a JSON line.
    pub fn record(&self, record: &Abw34Record) -> io::Result<()> {
        let mut guard = self.writer.lock().unwrap();
        serde_json::to_writer(&mut *guard, record)?;
        guard.write_all(b"\n")?;
        guard.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_jsonl_records() {
        let sink = Vec::new();
        let logger = Abw34Logger::from_writer(Box::new(sink));

        let record = Abw34Record {
            timestamp_ms: 1_700_000_000_000,
            qrng_source: "sandbox".into(),
            qrng_epoch: 7,
            qrng_tuple_id: "tuple/demo".into(),
            qrng_seed_hex: "abcd".into(),
            qrng_bits: 3072,
            qrng_shard_id: 42,
            shard_count: 10,
            noise_ratio: 0.5,
            qace_reroutes: 2,
            observed_tps_per_shard: 1_500_000.0,
            observed_tps_global: 15_000_000.0,
            kem_key_id: "kem".into(),
            signing_key_id: "sig".into(),
            kem_created_at: 111,
            kem_expires_at: 222,
            chsh_two_qubit_exact: Some(2.64),
            chsh_two_qubit_sampled: Some(2.60),
            chsh_five_d_exact: None,
            chsh_five_d_sampled: None,
            chsh_shots: Some(4096),
            chsh_depolarizing: Some(0.05),
        };

        logger.record(&record).expect("record");
    }
}
