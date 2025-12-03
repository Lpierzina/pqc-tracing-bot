use rand::{distributions::Distribution, Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use rand_distr::Normal;
use serde::{Deserialize, Serialize};

use crate::config::PrivacyPrimitiveConfig;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NoiseSummary {
    pub gaussian_sample: f64,
    pub laplace_sample: f64,
    pub renyi_epsilon: f64,
    pub epsilon_spent: f64,
    pub delta_spent: f64,
}

pub struct NoiseInjector {
    config: PrivacyPrimitiveConfig,
    rng: ChaCha20Rng,
}

impl NoiseInjector {
    pub fn new(config: PrivacyPrimitiveConfig, seed: [u8; 32]) -> Self {
        Self {
            config,
            rng: ChaCha20Rng::from_seed(seed),
        }
    }

    pub fn inject(&mut self, epsilon: f64, delta: f64) -> NoiseSummary {
        let gaussian = self.sample_gaussian(epsilon);
        let laplace = self.sample_laplace(epsilon);
        let renyi = self.renyi_epsilon(epsilon);
        NoiseSummary {
            gaussian_sample: gaussian,
            laplace_sample: laplace,
            renyi_epsilon: renyi,
            epsilon_spent: epsilon,
            delta_spent: delta,
        }
    }

    fn sample_gaussian(&mut self, epsilon: f64) -> f64 {
        let sigma = self.config.noise_sigma * (self.config.gaussian_epsilon / epsilon.max(1e-9));
        let normal = Normal::new(0.0, sigma.max(1e-9)).unwrap();
        normal.sample(&mut self.rng)
    }

    fn sample_laplace(&mut self, epsilon: f64) -> f64 {
        let scale = 1.0 / epsilon.max(self.config.laplace_epsilon);
        let uniform: f64 = self.rng.gen::<f64>() - 0.5;
        let sign = if uniform >= 0.0 { 1.0 } else { -1.0 };
        let magnitude = (1.0 - 2.0 * uniform.abs()).abs().max(1e-12).ln();
        -scale * sign * magnitude
    }

    fn renyi_epsilon(&self, epsilon: f64) -> f64 {
        (epsilon / self.config.gaussian_epsilon) * self.config.renyi_epsilon
    }
}
