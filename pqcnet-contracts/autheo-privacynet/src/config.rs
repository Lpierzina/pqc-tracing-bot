use autheo_pqcnet_5dezph::config::EzphConfig;
use serde::{Deserialize, Serialize};

use crate::{chaos::ChaosOracleConfig, dp::DpEngineConfig, fhe::FheLayerConfig};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrivacyNetConfig {
    pub ezph: EzphConfig,
    pub fhe: FheLayerConfig,
    pub dp: DpEngineConfig,
    pub budget: BudgetConfig,
    pub chaos: ChaosOracleConfig,
    pub api: ApiConfig,
}

impl Default for PrivacyNetConfig {
    fn default() -> Self {
        let mut ezph = EzphConfig::default();
        ezph.privacy.max_entropy_leak_bits = 1e-6;
        ezph.privacy.reyni_alpha = 1.25;
        ezph.manifold.projection_rank = 5;
        Self {
            ezph,
            fhe: FheLayerConfig::default(),
            dp: DpEngineConfig::default(),
            budget: BudgetConfig::default(),
            chaos: ChaosOracleConfig::default(),
            api: ApiConfig::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BudgetConfig {
    pub session_epsilon: f64,
    pub session_delta: f64,
    pub max_queries_per_session: u32,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            session_epsilon: 1e-5,
            session_delta: 2f64.powi(-40),
            max_queries_per_session: 10_000,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiConfig {
    pub max_payload_bytes: usize,
    pub max_public_inputs: usize,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            max_payload_bytes: 6_144,
            max_public_inputs: 32,
        }
    }
}
