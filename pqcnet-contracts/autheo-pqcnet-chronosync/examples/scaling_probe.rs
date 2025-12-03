use anyhow::{Context, Result};
use autheo_pqcnet_chronosync::ChronosyncConfig;
use clap::Parser;
use pqcnet_telemetry::abw34::{Abw34Logger, Abw34Record};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Parser, Debug)]
#[command(
    name = "scaling_probe",
    about = "Prototype Chronosync shard scaling with QRNG + ABW34 telemetry"
)]
struct Args {
    #[arg(
        long,
        default_value = "pqcnet-contracts/configs/chronosync-shards.toml",
        help = "Path to the shard profile TOML"
    )]
    config: PathBuf,
    #[arg(long, help = "Optional ABW34 log output path")]
    abw34_log: Option<PathBuf>,
    #[arg(long, help = "Optional JSON report path")]
    report_json: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct ScalingConfig {
    #[serde(default, rename = "profile")]
    profiles: Vec<ShardProfile>,
}

#[derive(Debug, Deserialize)]
struct ShardProfile {
    name: String,
    shards: u16,
    target_tps_global: u64,
    #[serde(default)]
    noise_ratio: f32,
    #[serde(default)]
    qace_reroutes: u32,
    #[serde(default = "default_qrng_source")]
    qrng_source: String,
}

fn default_qrng_source() -> String {
    "sandbox".into()
}

#[derive(Debug, Serialize)]
struct ProfileMeasurement<'a> {
    name: &'a str,
    shards: u16,
    target_tps_global: u64,
    observed_tps_per_shard: f64,
    noise_ratio: f32,
    qace_reroutes: u32,
    qrng_source: &'a str,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let bytes = fs::read(&args.config)
        .with_context(|| format!("failed to read {}", args.config.display()))?;
    let config: ScalingConfig =
        toml::from_str(&String::from_utf8(bytes).context("profile TOML is not valid UTF-8")?)
            .context("failed to parse shard profile TOML")?;
    if config.profiles.is_empty() {
        anyhow::bail!("no profiles defined in {}", args.config.display());
    }

    let logger = if let Some(path) = &args.abw34_log {
        Some(
            Abw34Logger::to_path(path)
                .with_context(|| format!("failed to open {}", path.display()))?,
        )
    } else {
        None
    };

    let mut report = Vec::new();
    for profile in &config.profiles {
        let per_shard = profile.target_tps_global as f64 / profile.shards as f64;
        println!(
            "[{}] shards={} global_tps={} per_shard_tps={:.0} noise={:.2} reroutes={}",
            profile.name,
            profile.shards,
            profile.target_tps_global,
            per_shard,
            profile.noise_ratio,
            profile.qace_reroutes
        );

        if let Some(logger) = &logger {
            logger
                .record(&Abw34Record {
                    timestamp_ms: unix_ms(),
                    qrng_source: profile.qrng_source.clone(),
                    qrng_epoch: 0,
                    qrng_tuple_id: format!("profile/{}", profile.name),
                    qrng_seed_hex: "placeholder".into(),
                    qrng_bits: 0,
                    qrng_shard_id: 0,
                    shard_count: profile.shards,
                    noise_ratio: clamp01(profile.noise_ratio),
                    qace_reroutes: profile.qace_reroutes,
                    observed_tps_per_shard: per_shard,
                    observed_tps_global: profile.target_tps_global as f64,
                    kem_key_id: "pending".into(),
                    signing_key_id: "pending".into(),
                    kem_created_at: 0,
                    kem_expires_at: 0,
                    chsh_two_qubit_exact: None,
                    chsh_two_qubit_sampled: None,
                    chsh_five_d_exact: None,
                    chsh_five_d_sampled: None,
                    chsh_shots: None,
                    chsh_depolarizing: None,
                })
                .context("failed to write ABW34 log")?;
        }

        let measurement = ProfileMeasurement {
            name: &profile.name,
            shards: profile.shards,
            target_tps_global: profile.target_tps_global,
            observed_tps_per_shard: per_shard,
            noise_ratio: profile.noise_ratio,
            qace_reroutes: profile.qace_reroutes,
            qrng_source: &profile.qrng_source,
        };
        report.push(measurement);

        let mut chrono_cfg = ChronosyncConfig::default();
        chrono_cfg.shards = profile.shards;
        chrono_cfg.global_tps = profile.target_tps_global;
        println!(
            "    -> Chronosync layers={} pools={} subpool_size={}",
            chrono_cfg.layers, chrono_cfg.verification_pools, chrono_cfg.subpool_size
        );
    }

    if let Some(path) = &args.report_json {
        let json = json!({ "profiles": report });
        fs::write(path, serde_json::to_string_pretty(&json)?)
            .with_context(|| format!("failed to write {}", path.display()))?;
        println!("wrote report to {}", path.display());
    }

    Ok(())
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|dur| dur.as_millis())
        .unwrap_or_default()
}

fn clamp01(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}
