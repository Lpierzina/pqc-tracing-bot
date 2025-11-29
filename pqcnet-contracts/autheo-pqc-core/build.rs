use serde::Deserialize;
use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

fn main() {
    link_windows();
    if let Err(err) = generate_recorded_trace() {
        panic!("failed to embed QFKH trace: {err}");
    }
}

fn link_windows() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "windows" {
        // `liboqs` pulls in Windows CryptoAPI symbols; ensure we link `Advapi32`.
        println!("cargo:rustc-link-lib=Advapi32");
    }
}

fn generate_recorded_trace() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let local_trace_path = manifest_dir.join("data/qfkh_prod_trace.json");
    let local_trace = fs::read_to_string(&local_trace_path)?;
    verify_workspace_trace(&manifest_dir, &local_trace);

    let trace: ProdTrace = serde_json::from_str(&local_trace)?;
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let out_path = out_dir.join("recorded_trace.rs");
    let mut file = File::create(out_path)?;
    writeln!(
        file,
        "const TRACE_ROTATION_INTERVAL_MS: u64 = {};",
        trace.config.rotation_interval_ms
    )?;
    writeln!(
        file,
        "static RECORDED_SAMPLES: [RecordedSample; {}] = [",
        trace.samples.len()
    )?;
    for sample in trace.samples {
        let public_key = decode_hex::<32>(&sample.public_key_hex)?;
        let secret_key = decode_hex::<32>(&sample.commitment_hex)?;
        let ciphertext = decode_hex::<48>(&sample.ciphertext_hex)?;
        let shared_secret = decode_hex::<32>(&sample.derived_key_hex)?;
        writeln!(
            file,
            "    RecordedSample {{ offset_ms: {}, public_key: {}, secret_key: {}, ciphertext: {}, shared_secret: {} }},",
            sample.window_start_ms,
            format_byte_array(&public_key),
            format_byte_array(&secret_key),
            format_byte_array(&ciphertext),
            format_byte_array(&shared_secret),
        )?;
    }
    writeln!(file, "];")?;
    Ok(())
}

fn verify_workspace_trace(manifest_dir: &Path, local_contents: &str) {
    let workspace_trace_path = manifest_dir
        .join("..")
        .join("pqcnet-qfkh")
        .join("data")
        .join("qfkh_prod_trace.json");
    if let Ok(workspace_contents) = fs::read_to_string(&workspace_trace_path) {
        if workspace_contents != local_contents {
            panic!(
                "autheo-pqc-core/data/qfkh_prod_trace.json is out of sync with pqcnet-qfkh/data/qfkh_prod_trace.json"
            );
        }
    }
}

#[derive(Deserialize)]
struct ProdTrace {
    config: TraceConfig,
    samples: Vec<TraceSample>,
}

#[derive(Deserialize)]
struct TraceConfig {
    rotation_interval_ms: u64,
}

#[derive(Deserialize)]
struct TraceSample {
    window_start_ms: u64,
    public_key_hex: String,
    ciphertext_hex: String,
    commitment_hex: String,
    derived_key_hex: String,
}

fn decode_hex<const N: usize>(input: &str) -> Result<[u8; N], Box<dyn std::error::Error>> {
    let bytes = hex::decode(input)?;
    if bytes.len() != N {
        return Err(format!("expected {N} bytes but received {}", bytes.len()).into());
    }
    let mut out = [0u8; N];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn format_byte_array(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 6 + 2);
    out.push('[');
    for (idx, byte) in bytes.iter().enumerate() {
        if idx > 0 {
            out.push_str(", ");
        }
        write!(&mut out, "0x{byte:02x}").expect("format write");
    }
    out.push(']');
    out
}
