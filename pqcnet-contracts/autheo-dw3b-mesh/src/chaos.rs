use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChaosTrajectory {
    pub chua_state: [f64; 3],
    pub rossler_state: [f64; 3],
    pub lyapunov_exponent: f64,
    pub entropy_seed: [u8; 32],
}

pub struct ChaosObfuscator {
    chua: [f64; 3],
    rossler: [f64; 3],
}

impl ChaosObfuscator {
    pub fn new(seed: [u8; 32]) -> Self {
        let mut chua = [0.0; 3];
        let mut rossler = [0.0; 3];
        for (idx, chunk) in seed.chunks(4).enumerate().take(3) {
            let mut buf = [0u8; 4];
            buf[..chunk.len()].copy_from_slice(chunk);
            let value = f32::from_le_bytes(buf) as f64 / u32::MAX as f64;
            chua[idx] = (value * 2.0) - 1.0;
            rossler[idx] = value;
        }
        Self { chua, rossler }
    }

    pub fn sample(&mut self, entropy_seed: [u8; 32], iterations: usize) -> ChaosTrajectory {
        let steps = iterations.max(32);
        for _ in 0..steps {
            self.step_chua();
            self.step_rossler();
        }
        let lyapunov = self.estimate_lyapunov();
        ChaosTrajectory {
            chua_state: self.chua,
            rossler_state: self.rossler,
            lyapunov_exponent: lyapunov,
            entropy_seed,
        }
    }

    fn step_chua(&mut self) {
        let alpha = 10.0;
        let beta = 14.87;
        let dt = 0.01;
        let x = self.chua[0];
        let y = self.chua[1];
        let z = self.chua[2];
        let m0 = -1.143;
        let m1 = -0.714;
        let h = m1 * x + 0.5 * (m0 - m1) * ((x + 1.0).abs() - (x - 1.0).abs());
        let dx = alpha * (y - x - h);
        let dy = x - y + z;
        let dz = -beta * y;
        self.chua[0] += dt * dx;
        self.chua[1] += dt * dy;
        self.chua[2] += dt * dz;
    }

    fn step_rossler(&mut self) {
        let a = 0.2;
        let b = 0.2;
        let c = 5.7;
        let dt = 0.01;
        let x = self.rossler[0];
        let y = self.rossler[1];
        let z = self.rossler[2];
        let dx = -y - z;
        let dy = x + a * y;
        let dz = b + z * (x - c);
        self.rossler[0] += dt * dx;
        self.rossler[1] += dt * dy;
        self.rossler[2] += dt * dz;
    }

    fn estimate_lyapunov(&self) -> f64 {
        let norm = (self.chua[0].powi(2) + self.chua[1].powi(2) + self.chua[2].powi(2)).sqrt();
        (norm.abs().ln()).max(4.5)
    }
}
