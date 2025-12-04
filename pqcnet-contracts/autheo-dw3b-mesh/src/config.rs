use autheo_pqcnet_5dezph::config::{FheBackendKind, ZkProverKind};
use autheo_privacynet::config::PrivacyNetConfig;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MeshNodeWeights {
    pub query: f32,
    pub mixnet: f32,
    pub stake: f32,
    pub index: f32,
    pub cdn: f32,
    pub governance: f32,
    pub key_management: f32,
    pub micro: f32,
}

impl Default for MeshNodeWeights {
    fn default() -> Self {
        Self {
            query: 0.18,
            mixnet: 0.22,
            stake: 0.12,
            index: 0.14,
            cdn: 0.10,
            governance: 0.08,
            key_management: 0.10,
            micro: 0.06,
        }
    }
}

impl MeshNodeWeights {
    pub fn normalized(&self) -> Self {
        let total = self.query
            + self.mixnet
            + self.stake
            + self.index
            + self.cdn
            + self.governance
            + self.key_management
            + self.micro;
        if total == 0.0 {
            return Self::default();
        }
        let scale = 1.0 / total;
        Self {
            query: self.query * scale,
            mixnet: self.mixnet * scale,
            stake: self.stake * scale,
            index: self.index * scale,
            cdn: self.cdn * scale,
            governance: self.governance * scale,
            key_management: self.key_management * scale,
            micro: self.micro * scale,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrivacyPrimitiveConfig {
    pub gaussian_epsilon: f64,
    pub gaussian_delta: f64,
    pub renyi_alpha: f64,
    pub renyi_epsilon: f64,
    pub laplace_epsilon: f64,
    pub noise_sigma: f64,
    pub fhe_depth: u32,
    pub fhe_scale: f64,
    pub halo2_verify_target_ms: u64,
    pub risc0_verify_target_ms: u64,
}

impl Default for PrivacyPrimitiveConfig {
    fn default() -> Self {
        Self {
            gaussian_epsilon: 1e-6,
            gaussian_delta: 2f64.powi(-40),
            renyi_alpha: 8.0,
            renyi_epsilon: 1e-5,
            laplace_epsilon: 1e-4,
            noise_sigma: 0.75,
            fhe_depth: 20,
            fhe_scale: 1.0e11,
            halo2_verify_target_ms: 8,
            risc0_verify_target_ms: 12,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuantumEntropyConfig {
    pub dimension: u8,
    pub samples_per_request: u32,
    pub amplification_target: f64,
    pub vrb_size_bits: u16,
}

impl Default for QuantumEntropyConfig {
    fn default() -> Self {
        Self {
            dimension: 5,
            samples_per_request: 64,
            amplification_target: 1e-154,
            vrb_size_bits: 512,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Dw3bMeshConfig {
    pub privacy: PrivacyNetConfig,
    pub primitives: PrivacyPrimitiveConfig,
    pub mesh_weights: MeshNodeWeights,
    pub entropy: QuantumEntropyConfig,
    #[serde(default)]
    pub zk_prover: ZkProverKind,
}

impl Dw3bMeshConfig {
    pub fn production() -> Self {
        let mut privacy = PrivacyNetConfig::default();
        privacy.ezph.zk_prover = ZkProverKind::Halo2;
        privacy.ezph.fhe_evaluator = FheBackendKind::Tfhe;
        Self {
            privacy,
            primitives: PrivacyPrimitiveConfig::default(),
            mesh_weights: MeshNodeWeights::default(),
            entropy: QuantumEntropyConfig::default(),
            zk_prover: ZkProverKind::Halo2,
        }
    }
}

impl Default for Dw3bMeshConfig {
    fn default() -> Self {
        Self::production()
    }
}
