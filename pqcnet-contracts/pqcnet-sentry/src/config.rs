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

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Config {
    pub sentry: SentrySection,
    pub crypto: CryptoConfig,
    pub networking: NetworkingConfig,
    pub telemetry: TelemetryConfig,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct SentrySection {
    #[serde(default)]
    pub watchers: Vec<String>,
    #[serde(default = "default_poll_interval_ms")]
    pub poll_interval_ms: u64,
    #[serde(default = "default_quorum_threshold")]
    pub quorum_threshold: u8,
}

const fn default_poll_interval_ms() -> u64 {
    2_000
}

const fn default_quorum_threshold() -> u8 {
    2
}

impl Config {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.sentry.watchers.is_empty() {
            return Err(ConfigError::Validation(
                "at least one watcher must be defined".into(),
            ));
        }
        if self.sentry.quorum_threshold == 0 {
            return Err(ConfigError::Validation(
                "quorum threshold must be greater than zero".into(),
            ));
        }
        Ok(())
    }

    pub fn sample() -> Self {
        Self {
            sentry: SentrySection {
                watchers: vec!["peer-a".into(), "peer-b".into()],
                poll_interval_ms: default_poll_interval_ms(),
                quorum_threshold: default_quorum_threshold(),
            },
            crypto: CryptoConfig::sample("sentry-a"),
            networking: NetworkingConfig::sample("0.0.0.0:7100"),
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
        ConfigFormat::Auto => unreachable!("auto variant resolved earlier"),
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
    fn detects_missing_watchers() {
        let mut config = Config::sample();
        config.sentry.watchers.clear();
        assert!(matches!(config.validate(), Err(ConfigError::Validation(_))));
    }

    #[test]
    fn parses_toml_config() {
        let contents = r#"
            [sentry]
            watchers = ["peer-a", "peer-b"]
            quorum-threshold = 2

            [crypto]
            node-id = "sentry-a"
            secret-seed = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"

            [networking]
            listen = "0.0.0.0:7100"

            [[networking.peers]]
            id = "peer-a"
            address = "127.0.0.1:7101"

            [telemetry]
            endpoint = "http://localhost:4318"
        "#;

        let config: Config = toml::from_str(contents).unwrap();
        assert_eq!(config.sentry.watchers.len(), 2);
    }

    #[test]
    fn parses_yaml_config() {
        let contents = r#"
            sentry:
              watchers: ["peer-a"]
            crypto:
              node-id: sentry-b
              secret-seed: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            networking:
              listen: "0.0.0.0:7200"
            telemetry:
              endpoint: http://localhost:4318
        "#;
        let config: Config = serde_yaml::from_str(contents).unwrap();
        assert_eq!(config.crypto.node_id, "sentry-b");
    }
}
