use std::io::{self, Cursor};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompressionReport {
    pub bytes_in: usize,
    pub bytes_out: usize,
    pub ratio: f64,
}

pub struct CompressionPipeline {
    level: i32,
}

impl CompressionPipeline {
    pub fn new(level: i32) -> Self {
        Self { level }
    }

    pub fn compress(&self, data: &[u8]) -> io::Result<(Vec<u8>, CompressionReport)> {
        let mut cursor = Cursor::new(data);
        let compressed = zstd::stream::encode_all(&mut cursor, self.level)?;
        let report = CompressionReport {
            bytes_in: data.len(),
            bytes_out: compressed.len(),
            ratio: if data.is_empty() {
                1.0
            } else {
                data.len() as f64 / compressed.len().max(1) as f64
            },
        };
        Ok((compressed, report))
    }
}
