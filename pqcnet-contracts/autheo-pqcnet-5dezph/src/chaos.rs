use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use rand_distr::StandardNormal;

use crate::config::ChaosConfig;

#[derive(Clone, Copy, Debug, Default)]
pub struct ChaosVector {
    pub lorenz: [f64; 3],
    pub chua: [f64; 3],
    pub logistic: f64,
    pub gaussian_noise: f64,
}

impl ChaosVector {
    pub fn energy(&self) -> f64 {
        self.lorenz.iter().map(|v| v * v).sum::<f64>()
            + self.chua.iter().map(|v| v * v).sum::<f64>()
            + self.logistic.powi(2)
    }

    pub fn phase(&self) -> f64 {
        (self.lorenz[2] + self.chua[0] + self.logistic).fract()
    }
}

pub trait ChaosEngine: Send + Sync {
    fn sample(&self, seed: &[u8; 32]) -> ChaosVector;
}

pub struct LorenzChuaChaos {
    config: ChaosConfig,
    dt: f64,
}

impl LorenzChuaChaos {
    pub fn new(config: ChaosConfig) -> Self {
        Self { config, dt: 0.01 }
    }

    fn evolve_lorenz(&self, seed: &[u8; 32]) -> [f64; 3] {
        let mut rng = ChaCha20Rng::from_seed(*seed);
        let mut x = rng.gen_range(-10.0..10.0);
        let mut y = rng.gen_range(-10.0..10.0);
        let mut z = rng.gen_range(15.0..35.0);
        for _ in 0..self.config.iterations {
            let dx = self.config.lorenz_sigma * (y - x);
            let dy = x * (self.config.lorenz_rho - z) - y;
            let dz = x * y - self.config.lorenz_beta * z;
            x += dx * self.dt;
            y += dy * self.dt;
            z += dz * self.dt;
        }
        [x, y, z]
    }

    fn evolve_chua(&self, seed: &[u8; 32]) -> [f64; 3] {
        let mut rng = ChaCha20Rng::from_seed(*seed);
        let mut x = rng.gen_range(-2.0..2.0);
        let mut y = rng.gen_range(-2.0..2.0);
        let mut z = rng.gen_range(-2.0..2.0);
        const M0: f64 = -1.143;
        const M1: f64 = -0.714;
        for _ in 0..self.config.iterations {
            let f = M1 * x + 0.5 * (M0 - M1) * ((x + 1.0).abs() - (x - 1.0).abs());
            let dx = self.config.chua_alpha * (y - x - f);
            let dy = x - y + z;
            let dz = -self.config.chua_beta * y - self.config.chua_gamma * z;
            x += dx * self.dt;
            y += dy * self.dt;
            z += dz * self.dt;
        }
        [x, y, z]
    }

    fn logistic(&self, seed: &[u8; 32]) -> (f64, f64) {
        let mut rng = ChaCha20Rng::from_seed(*seed);
        let mut x = rng.gen_range(0.25..0.75);
        for _ in 0..self.config.iterations {
            x = self.config.logistic_r * x * (1.0 - x);
        }
        let noise = rng.sample::<f64, _>(StandardNormal);
        (x.clamp(0.0, 1.0), noise)
    }
}

impl ChaosEngine for LorenzChuaChaos {
    fn sample(&self, seed: &[u8; 32]) -> ChaosVector {
        let mut lorenz_seed = [0u8; 32];
        let mut chua_seed = [0u8; 32];
        let mut logistic_seed = [0u8; 32];
        lorenz_seed.copy_from_slice(seed);
        chua_seed.copy_from_slice(seed);
        logistic_seed.copy_from_slice(seed);
        lorenz_seed[0] ^= 0xA5;
        chua_seed[1] ^= 0x5A;
        logistic_seed[2] ^= 0x3C;
        let lorenz = self.evolve_lorenz(&lorenz_seed);
        let chua = self.evolve_chua(&chua_seed);
        let (logistic, gaussian_noise) = self.logistic(&logistic_seed);
        ChaosVector {
            lorenz,
            chua,
            logistic,
            gaussian_noise,
        }
    }
}
