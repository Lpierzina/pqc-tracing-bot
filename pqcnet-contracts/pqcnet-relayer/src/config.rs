use std::{
    fs,
    path::{Path, PathBuf},
};

use clap::ValueEnum;
use pqcnet_crypto::CryptoConfig;
use pqcnet_networking::NetworkingConfig;
use pqcnet_telemetry::TelemetryConfig;
use serde::Deserialize;
use thiserror::Error;

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum ConfigFormat {
    Auto,
    Toml,
    Yaml,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RelayerMode {
    Ingest,
    Egress,
    Bidirectional,
}

impl RelayerMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            RelayerMode::Ingest => "ingest",
            RelayerMode::Egress => "egress",
            RelayerMode::Bidirectional => "bidirectional",
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("unable to read config {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse {format:?} config: {details}")]
    Parse {
        format: ConfigFormat,
        details: String,
    },
    #[error("configuration invalid: {0}")]
    Validation(String),
}

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub relayer: RelayerSection,
    pub crypto: CryptoConfig,
    pub networking: NetworkingConfig,
    pub telemetry: TelemetryConfig,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct RelayerSection {
    #[serde(default = "default_batch_size")]
    pub batch_size: u16,
    #[serde(default = "default_max_queue_depth")]
    pub max_queue_depth: u32,
    #[serde(default = "default_retry_backoff_ms")]
    pub retry_backoff_ms: u64,
    #[serde(default = "default_mode")]
    pub mode: RelayerMode,
}

const fn default_batch_size() -> u16 {
    8
}

const fn default_max_queue_depth() -> u32 {
    2048
}

const fn default_retry_backoff_ms() -> u64 {
    500
}

const fn default_mode() -> RelayerMode {
    RelayerMode::Bidirectional
}

impl Config {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.relayer.batch_size == 0 {
            return Err(ConfigError::Validation(
                "batch size must be greater than zero".into(),
            ));
        }
        if self.relayer.batch_size as u32 > self.relayer.max_queue_depth {
            return Err(ConfigError::Validation(
                "batch size cannot exceed max queue depth".into(),
            ));
        }
        Ok(())
    }

    pub fn sample() -> Self {
        Self {
            relayer: RelayerSection {
                batch_size: default_batch_size(),
                max_queue_depth: default_max_queue_depth(),
                retry_backoff_ms: default_retry_backoff_ms(),
                mode: RelayerMode::Bidirectional,
            },
            crypto: CryptoConfig::sample("relayer-a"),
            networking: NetworkingConfig::sample("0.0.0.0:7300"),
            telemetry: TelemetryConfig::sample("http://localhost:4318"),
        }
    }
}

pub fn load_config(path: &Path, format: ConfigFormat) -> Result<Config, ConfigError> {
    let contents = fs::read_to_string(path).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let format = resolve_format(path, format);
    let config: Config = match format {
        ConfigFormat::Toml => toml::from_str(&contents).map_err(|err| ConfigError::Parse {
            format,
            details: err.to_string(),
        }),
        ConfigFormat::Yaml => serde_yaml::from_str(&contents).map_err(|err| ConfigError::Parse {
            format,
            details: err.to_string(),
        }),
        ConfigFormat::Auto => unreachable!(),
    }?;
    config.validate()?;
    Ok(config)
}

fn resolve_format(path: &Path, format: ConfigFormat) -> ConfigFormat {
    match format {
        ConfigFormat::Auto => match path.extension().and_then(|ext| ext.to_str()) {
            Some("toml") => ConfigFormat::Toml,
            Some("yaml") | Some("yml") => ConfigFormat::Yaml,
            _ => ConfigFormat::Toml,
        },
        _ => format,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_config_is_valid() {
        Config::sample().validate().unwrap();
    }
}
