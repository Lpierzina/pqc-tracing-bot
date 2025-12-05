use autheo_pqcnet_5dqeh::QehConfig;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Composite configuration for the 5D-EZPH orchestrator.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EzphConfig {
    pub qeh: QehConfig,
    pub manifold: ManifoldConfig,
    pub chaos: ChaosConfig,
    pub privacy: PrivacyBounds,
    pub zk: ZkConfig,
    pub fhe: FheConfig,
    #[serde(default)]
    pub zk_prover: ZkProverKind,
    #[serde(default)]
    pub fhe_evaluator: FheBackendKind,
}

impl Default for EzphConfig {
    fn default() -> Self {
        Self {
            qeh: QehConfig::default(),
            manifold: ManifoldConfig::default(),
            chaos: ChaosConfig::default(),
            privacy: PrivacyBounds::default(),
            zk: ZkConfig::default(),
            fhe: FheConfig::default(),
            zk_prover: ZkProverKind::default(),
            fhe_evaluator: FheBackendKind::default(),
        }
    }
}

impl EzphConfig {
    /// Override the inner 5D-QEH configuration while retaining EZPH defaults.
    pub fn with_qeh(mut self, qeh: QehConfig) -> Self {
        self.qeh = qeh;
        self
    }

    pub fn with_backends(mut self, zk: ZkProverKind, fhe: FheBackendKind) -> Self {
        self.zk_prover = zk;
        self.fhe_evaluator = fhe;
        self
    }
}

/// Selects which ZK prover backend the EZPH pipeline should instantiate.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ZkProverKind {
    MockCircom,
    Halo2,
}

impl Default for ZkProverKind {
    fn default() -> Self {
        Self::Halo2
    }
}

/// Selects which FHE evaluator backend powers the EZPH slots.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum FheBackendKind {
    MockCkks,
    Tfhe,
}

impl Default for FheBackendKind {
    fn default() -> Self {
        Self::Tfhe
    }
}

/// Manifold parameters describing how the five privacy dimensions are embedded.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ManifoldConfig {
    pub spatial_radius_mm: f64,
    pub entropy_register: usize,
    pub projection_rank: usize,
    pub homomorphic_scale: f64,
}

impl Default for ManifoldConfig {
    fn default() -> Self {
        Self {
            spatial_radius_mm: 7.5,
            entropy_register: 32,
            projection_rank: 3,
            homomorphic_scale: 1.0,
        }
    }
}

/// Chaos-system parameters (Lorenz + Chua + logistic attractors).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChaosConfig {
    pub lorenz_sigma: f64,
    pub lorenz_rho: f64,
    pub lorenz_beta: f64,
    pub chua_alpha: f64,
    pub chua_beta: f64,
    pub chua_gamma: f64,
    pub logistic_r: f64,
    pub iterations: usize,
}

impl Default for ChaosConfig {
    fn default() -> Self {
        Self {
            lorenz_sigma: 10.0,
            lorenz_rho: 28.0,
            lorenz_beta: 8.0 / 3.0,
            chua_alpha: 15.6,
            chua_beta: 28.0,
            chua_gamma: 0.1,
            logistic_r: 3.999,
            iterations: 128,
        }
    }
}

/// Privacy bounds for EZPH metrics.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrivacyBounds {
    pub max_entropy_leak_bits: f64,
    pub min_reyni_divergence: f64,
    pub hockey_stick_delta: f64,
    pub amplification_gain: f64,
    pub reyni_alpha: f64,
}

impl Default for PrivacyBounds {
    fn default() -> Self {
        Self {
            max_entropy_leak_bits: 1e-6,
            min_reyni_divergence: 42.0,
            hockey_stick_delta: 1e-12,
            amplification_gain: 154.0,
            reyni_alpha: 1.25,
        }
    }
}

/// Zero-knowledge prover configuration (circuit metadata + target soundness).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ZkConfig {
    pub circuit_id: String,
    pub soundness: f64,
    pub curve: String,
    pub proof_system: String,
    pub params_path: PathBuf,
    pub proving_key_path: PathBuf,
    pub verifying_key_path: PathBuf,
}

impl Default for ZkConfig {
    fn default() -> Self {
        Self {
            circuit_id: "autheo/ezph/kyc-age".into(),
            soundness: 2f64.powi(-256),
            curve: "BLS12-381".into(),
            proof_system: "Halo2".into(),
            params_path: PathBuf::from("config/crypto/halo2.params"),
            proving_key_path: PathBuf::from("config/crypto/halo2.pk"),
            verifying_key_path: PathBuf::from("config/crypto/halo2.vk"),
        }
    }
}

/// CKKS-style FHE tuning knobs.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FheConfig {
    pub polynomial_degree: usize,
    pub ciphertext_scale: f64,
}

impl Default for FheConfig {
    fn default() -> Self {
        Self {
            polynomial_degree: 8192,
            ciphertext_scale: 2f64.powi(40),
        }
    }
}
