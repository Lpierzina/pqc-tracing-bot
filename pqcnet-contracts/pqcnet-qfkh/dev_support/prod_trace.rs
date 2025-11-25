use serde::Deserialize;

const TRACE_JSON: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/data/qfkh_prod_trace.json"));

#[derive(Debug, Deserialize)]
pub struct TraceConfig {
    pub rotation_interval_ms: u64,
    pub lookahead_epochs: u32,
}

#[derive(Debug, Deserialize)]
pub struct HopSample {
    pub epoch: u64,
    pub announce_at_ms: u64,
    pub activate_at_ms: u64,
    pub window_start_ms: u64,
    pub window_end_ms: u64,
    pub key_id_hex: String,
    pub public_key_hex: String,
    pub ciphertext_hex: String,
    pub commitment_hex: String,
    pub derived_key_hex: String,
}

#[derive(Debug, Deserialize)]
pub struct ProdTrace {
    pub config: TraceConfig,
    pub samples: Vec<HopSample>,
}

impl ProdTrace {
    pub fn load() -> Self {
        serde_json::from_str(TRACE_JSON).expect("valid prod trace JSON")
    }
}

pub fn hex_to_array<const N: usize>(hex: &str) -> [u8; N] {
    let bytes = hex::decode(hex).expect("hex decode");
    assert_eq!(bytes.len(), N, "expected {N} bytes, got {}", bytes.len());
    let mut out = [0u8; N];
    out.copy_from_slice(&bytes);
    out
}

pub fn hex_to_vec(hex: &str) -> Vec<u8> {
    hex::decode(hex).expect("hex decode")
}
