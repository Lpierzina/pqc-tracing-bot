use std::path::PathBuf;

use clap::Parser;
use color_eyre::Result;
use pqcnet_crypto::CryptoProvider;
use pqcnet_networking::NetworkClient;
use pqcnet_sentry::config::{load_config, ConfigFormat};
use pqcnet_sentry::service::SentryService;
use pqcnet_telemetry::TelemetryHandle;

#[cfg(any(
    all(feature = "dev", feature = "test"),
    all(feature = "dev", feature = "prod"),
    all(feature = "test", feature = "prod")
))]
compile_error!(
    "Only one of the `dev`, `test`, or `prod` features may be enabled for pqcnet-sentry."
);

#[derive(Debug, Parser)]
#[command(
    name = "pqcnet-sentry",
    version,
    about = "Reference sentry daemon that watches PQC relayers"
)]
struct Cli {
    /// Path to configuration file (TOML or YAML).
    #[arg(long, default_value = "configs/pqcnet-sentry.toml")]
    config: PathBuf,
    /// Explicit configuration format override.
    #[arg(long, value_enum, default_value_t = ConfigFormat::Auto)]
    config_format: ConfigFormat,
    /// Number of iterations to execute before exiting.
    #[arg(long, default_value_t = 1)]
    iterations: u16,
    /// Skip network calls and only emit telemetry.
    #[arg(long)]
    dry_run: bool,
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();
    let config = load_config(&cli.config, cli.config_format)?;
    let crypto = CryptoProvider::from_config(&config.crypto)?;
    let network = NetworkClient::from_config(&config.crypto.node_id, config.networking.clone());
    let telemetry = TelemetryHandle::from_config(config.telemetry.clone());
    let mut service = SentryService::new(&config, crypto, network, telemetry);

    for iteration in 0..cli.iterations {
        let report = service.run_iteration(cli.dry_run)?;
        println!(
            "iteration {} processed {} watchers (quorum {})",
            iteration + 1,
            report.processed_watchers,
            report.quorum_threshold
        );
    }

    Ok(())
}
