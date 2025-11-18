mod config;
mod service;

use std::path::PathBuf;

use clap::Parser;
use color_eyre::Result;
use config::{load_config, ConfigError, ConfigFormat, RelayerMode};
use pqcnet_crypto::CryptoProvider;
use pqcnet_networking::NetworkClient;
use pqcnet_telemetry::TelemetryHandle;
use service::RelayerService;

#[cfg(any(
    all(feature = "dev", feature = "test"),
    all(feature = "dev", feature = "prod"),
    all(feature = "test", feature = "prod")
))]
compile_error!(
    "Only one of the `dev`, `test`, or `prod` features may be enabled for pqcnet-relayer."
);

#[derive(Debug, Parser)]
#[command(
    name = "pqcnet-relayer",
    version,
    about = "Reference relayer daemon that shuttles PQC messages between peers"
)]
struct Cli {
    /// Path to configuration file (TOML or YAML).
    #[arg(long, default_value = "configs/pqcnet-relayer.toml")]
    config: PathBuf,
    /// Explicit configuration format override.
    #[arg(long, value_enum, default_value_t = ConfigFormat::Auto)]
    config_format: ConfigFormat,
    /// Override the relayer mode defined in the config file.
    #[arg(long, value_enum)]
    mode: Option<RelayerMode>,
    /// Override the batch size defined in the config file.
    #[arg(long)]
    batch_size: Option<u16>,
    /// Number of iterations to execute.
    #[arg(long, default_value_t = 1)]
    iterations: u16,
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();
    let mut config = load_config(&cli.config, cli.config_format)?;
    if let Some(mode) = cli.mode {
        config.relayer.mode = mode;
    }
    if let Some(batch) = cli.batch_size {
        config.relayer.batch_size = batch;
    }
    config.validate().map_err(|err| match err {
        ConfigError::Validation(reason) => color_eyre::eyre::eyre!(reason),
        other => other.into(),
    })?;

    let crypto = CryptoProvider::from_config(&config.crypto)?;
    let network = NetworkClient::from_config(&config.crypto.node_id, config.networking.clone());
    let telemetry = TelemetryHandle::from_config(config.telemetry.clone());
    let mut service = RelayerService::new(&config, crypto, network, telemetry);

    for iteration in 0..cli.iterations {
        let report = service.relay_once()?;
        println!(
            "iteration {} mode={} delivered={} buffered={}",
            iteration + 1,
            report.mode.as_str(),
            report.delivered,
            report.buffered
        );
    }

    Ok(())
}
