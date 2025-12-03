use autheo_pqcnet_5dezph::{fhe::FheCiphertext, zk::ZkProof};
use blake3::Hasher;
use rand::{rngs::StdRng, Rng, RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;
use rand_distr::{Distribution, Normal};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::chaos::ChaosSample;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Blake3Hash(pub [u8; 32]);

impl Blake3Hash {
    pub fn derive(label: impl AsRef<[u8]>) -> Self {
        let mut hasher = Hasher::new();
        hasher.update(label.as_ref());
        let mut out = [0u8; 32];
        out.copy_from_slice(hasher.finalize().as_bytes());
        Self(out)
    }

    pub fn random() -> Self {
        Self(random_seed())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum DpMechanism {
    Gaussian,
    Laplace,
    Exponential,
    Renyi { alpha: f64 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DpQuery {
    pub query_id: Blake3Hash,
    pub spatial_domain: Vec<u64>,
    pub sensitivity: f64,
    pub epsilon: f64,
    pub delta: f64,
    pub mechanism: DpMechanism,
    pub noise_seed: [u8; 32],
    pub composition_id: u64,
    pub fhe_context: FheCiphertext,
    pub zk_proof: ZkProof,
}

impl DpQuery {
    pub fn gaussian(spatial_domain: Vec<u64>, epsilon: f64, delta: f64, sensitivity: f64) -> Self {
        Self {
            query_id: Blake3Hash::random(),
            spatial_domain,
            sensitivity,
            epsilon,
            delta,
            mechanism: DpMechanism::Gaussian,
            noise_seed: random_seed(),
            composition_id: 0,
            fhe_context: FheCiphertext::zero(0.0),
            zk_proof: ZkProof {
                proof_system: "halo2".into(),
                curve: "BLS12-381".into(),
                proof_bytes: vec![],
                statement_hash: [0u8; 32],
                soundness_error: 0.0,
            },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DpEngineConfig {
    pub gaussian_epsilon: f64,
    pub gaussian_delta: f64,
    pub laplace_epsilon: f64,
    pub exponential_epsilon: f64,
    pub renyi_alpha: f64,
    pub renyi_epsilon: f64,
}

impl Default for DpEngineConfig {
    fn default() -> Self {
        Self {
            gaussian_epsilon: 1e-6,
            gaussian_delta: 2f64.powi(-40),
            laplace_epsilon: 1e-4,
            exponential_epsilon: 1e-3,
            renyi_alpha: 8.0,
            renyi_epsilon: 1e-5,
        }
    }
}

#[derive(Debug, Error)]
pub enum DpError {
    #[error("epsilon must be > 0")]
    InvalidEpsilon,
    #[error("delta must be within (0,1)")]
    InvalidDelta,
    #[error("sensitivity must be positive")]
    InvalidSensitivity,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DpSample {
    pub query_id: Blake3Hash,
    pub mechanism: DpMechanism,
    pub noisy_vector: Vec<f64>,
    pub epsilon_spent: f64,
    pub delta_spent: f64,
    pub composition_id: u64,
    pub lyapunov_amplifier: f64,
}

pub struct DifferentialPrivacyEngine {
    config: DpEngineConfig,
    rng: ChaCha20Rng,
}

impl DifferentialPrivacyEngine {
    pub fn new(config: DpEngineConfig, seed: [u8; 32]) -> Self {
        Self {
            config,
            rng: ChaCha20Rng::from_seed(seed),
        }
    }

    pub fn execute(&mut self, query: &DpQuery, chaos: &ChaosSample) -> Result<DpSample, DpError> {
        if query.epsilon <= 0.0 {
            return Err(DpError::InvalidEpsilon);
        }
        if !(0.0..1.0).contains(&query.delta) {
            return Err(DpError::InvalidDelta);
        }
        if query.sensitivity <= 0.0 {
            return Err(DpError::InvalidSensitivity);
        }
        let lyapunov_gain = chaos.lyapunov_exponent.max(1.0);
        let noise = match query.mechanism {
            DpMechanism::Gaussian => {
                let sigma = self.gaussian_sigma(query, lyapunov_gain);
                self.sample_gaussian(query, sigma)
            }
            DpMechanism::Laplace => {
                let b = (query.sensitivity * lyapunov_gain)
                    / query.epsilon.max(self.config.laplace_epsilon);
                self.sample_laplace(query, b)
            }
            DpMechanism::Exponential => self.sample_exponential(query, lyapunov_gain),
            DpMechanism::Renyi { .. } => self.sample_renyi(query, lyapunov_gain),
        };
        Ok(DpSample {
            query_id: query.query_id,
            mechanism: query.mechanism.clone(),
            noisy_vector: noise,
            epsilon_spent: query.epsilon,
            delta_spent: query.delta,
            composition_id: query.composition_id,
            lyapunov_amplifier: lyapunov_gain,
        })
    }

    fn gaussian_sigma(&self, query: &DpQuery, lyapunov: f64) -> f64 {
        let epsilon = query.epsilon.max(self.config.gaussian_epsilon);
        let delta = query.delta.max(self.config.gaussian_delta);
        let numerator = query.sensitivity * (2.0 * lyapunov.ln().abs().max(1.0)).sqrt();
        (numerator / epsilon.max(1e-18)) / delta.max(1e-18)
    }

    fn sample_gaussian(&mut self, query: &DpQuery, sigma: f64) -> Vec<f64> {
        let normal = Normal::new(0.0, sigma.max(1e-18)).unwrap();
        query
            .spatial_domain
            .iter()
            .map(|value| *value as f64 + normal.sample(&mut self.rng))
            .collect()
    }

    fn sample_laplace(&mut self, query: &DpQuery, b: f64) -> Vec<f64> {
        query
            .spatial_domain
            .iter()
            .map(|value| {
                let u: f64 = self.rng.gen::<f64>() - 0.5;
                let sign = if u >= 0.0 { 1.0 } else { -1.0 };
                let magnitude = (1.0 - 2.0 * u.abs()).abs().max(1e-12);
                let noise = -b.max(1e-18) * sign * magnitude.ln();
                *value as f64 + noise
            })
            .collect()
    }

    fn sample_exponential(&mut self, query: &DpQuery, lyapunov: f64) -> Vec<f64> {
        if query.spatial_domain.is_empty() {
            return vec![0.0];
        }
        let beta = query.epsilon.max(self.config.exponential_epsilon)
            / (2.0 * query.sensitivity.max(1e-18) * lyapunov.max(1.0));
        let weights: Vec<f64> = query
            .spatial_domain
            .iter()
            .map(|candidate| (beta * *candidate as f64).exp())
            .collect();
        let total: f64 = weights.iter().sum::<f64>().max(1e-18);
        let choice = self.rng.gen::<f64>() * total;
        let mut accum = 0.0;
        let mut selected = query.spatial_domain[0] as f64;
        for (candidate, weight) in query.spatial_domain.iter().zip(weights.iter()) {
            accum += weight;
            if choice <= accum {
                selected = *candidate as f64;
                break;
            }
        }
        vec![selected]
    }

    fn sample_renyi(&mut self, query: &DpQuery, lyapunov: f64) -> Vec<f64> {
        let alpha = if let DpMechanism::Renyi { alpha } = query.mechanism {
            alpha
        } else {
            self.config.renyi_alpha
        };
        let scaling = ((alpha - 1.0) / alpha).max(1e-6);
        let sigma = (2.0 * lyapunov * scaling).sqrt() * query.sensitivity;
        let normal = Normal::new(0.0, sigma.max(1e-18)).unwrap();
        query
            .spatial_domain
            .iter()
            .map(|value| *value as f64 + normal.sample(&mut self.rng))
            .collect()
    }
}

fn random_seed() -> [u8; 32] {
    let mut seed = [0u8; 32];
    let mut rng = StdRng::from_entropy();
    rng.fill_bytes(&mut seed);
    seed
}
