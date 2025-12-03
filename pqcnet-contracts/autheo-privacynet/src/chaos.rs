use blake3::Hasher;
use rand::{rngs::StdRng, Rng, RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};

/// Configuration for the Chua/Rössler chaos oracle that feeds DP + EZPH layers.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChaosOracleConfig {
    pub chua_alpha: f64,
    pub chua_beta: f64,
    pub chua_gamma: f64,
    pub rossler_a: f64,
    pub rossler_b: f64,
    pub rossler_c: f64,
    pub lyapunov_floor: f64,
}

impl Default for ChaosOracleConfig {
    fn default() -> Self {
        Self {
            chua_alpha: 15.6,
            chua_beta: 28.0,
            chua_gamma: 0.1,
            rossler_a: 0.2,
            rossler_b: 0.2,
            rossler_c: 5.7,
            lyapunov_floor: 4.5,
        }
    }
}

/// Snapshot of the dual-attractor trajectories.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChaosSample {
    pub chua_state: [f64; 3],
    pub rossler_state: [f64; 3],
    pub lyapunov_exponent: f64,
    pub entropy_seed: [u8; 32],
}

/// Deterministic chaos oracle used by the PrivacyNet orchestrator.
pub struct ChaosOracle {
    config: ChaosOracleConfig,
    entropy: ChaCha20Rng,
}

impl ChaosOracle {
    pub fn with_seed(seed: [u8; 32], config: ChaosOracleConfig) -> Self {
        Self {
            config,
            entropy: ChaCha20Rng::from_seed(seed),
        }
    }

    pub fn new(config: ChaosOracleConfig) -> Self {
        let mut seed = [0u8; 32];
        let mut bootstrap = StdRng::from_entropy();
        bootstrap.fill(&mut seed);
        Self::with_seed(seed, config)
    }

    /// Emits a chaos sample and derives a new seed for downstream DP engines.
    pub fn sample(&mut self, request_seed: &[u8; 32]) -> ChaosSample {
        let mut entropy_tap = [0u8; 32];
        self.entropy.fill_bytes(&mut entropy_tap);
        let mut hasher = Hasher::new();
        hasher.update(&entropy_tap);
        hasher.update(request_seed);
        let mut derived_seed = [0u8; 32];
        derived_seed.copy_from_slice(hasher.finalize().as_bytes());
        self.entropy = ChaCha20Rng::from_seed(derived_seed);
        let chua = self.iterate_chua();
        let rossler = self.iterate_rossler();
        let lyapunov = self.estimate_lyapunov(&chua, &rossler);
        ChaosSample {
            chua_state: chua,
            rossler_state: rossler,
            lyapunov_exponent: lyapunov,
            entropy_seed: derived_seed,
        }
    }

    fn iterate_chua(&mut self) -> [f64; 3] {
        let ChaosOracleConfig {
            chua_alpha: alpha,
            chua_beta: beta,
            chua_gamma: gamma,
            ..
        } = self.config;
        let mut x = self.entropy.gen_range(-1.0..1.0);
        let mut y = self.entropy.gen_range(-1.0..1.0);
        let mut z = self.entropy.gen_range(-1.0..1.0);
        for _ in 0..64 {
            let fx = self.chua_non_linear(x);
            let dx = alpha * (y - x - fx);
            let dy = x - y + z;
            let dz = -beta * y - gamma * z;
            x += dx * 0.01;
            y += dy * 0.01;
            z += dz * 0.01;
        }
        [x, y, z]
    }

    fn iterate_rossler(&mut self) -> [f64; 3] {
        let ChaosOracleConfig {
            rossler_a: a,
            rossler_b: b,
            rossler_c: c,
            ..
        } = self.config;
        let mut x = self.entropy.gen_range(-1.0..1.0);
        let mut y = self.entropy.gen_range(-1.0..1.0);
        let mut z = self.entropy.gen_range(0.0..1.0);
        for _ in 0..64 {
            let dx = -y - z;
            let dy = x + a * y;
            let dz = b + z * (x - c);
            x += dx * 0.02;
            y += dy * 0.02;
            z += dz * 0.02;
        }
        [x, y, z]
    }

    fn estimate_lyapunov(&self, chua: &[f64; 3], rossler: &[f64; 3]) -> f64 {
        let chua_norm = chua.iter().map(|v| v.powi(2)).sum::<f64>().sqrt();
        let rossler_norm = rossler.iter().map(|v| v.powi(2)).sum::<f64>().sqrt();
        (chua_norm + rossler_norm).max(self.config.lyapunov_floor)
    }

    fn chua_non_linear(&self, x: f64) -> f64 {
        let m0 = -1.143;
        let m1 = -0.714;
        let term = (x + 1.0).abs() - (x - 1.0).abs();
        m1 * x + 0.5 * (m0 - m1) * term
    }
}
