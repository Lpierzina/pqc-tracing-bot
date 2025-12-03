use autheo_pqcnet_5dezph::fhe::{FheCiphertext, FheError, FheEvaluator, MockCkksEvaluator};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FheLayerConfig {
    pub ring_dimension: usize,
    pub bootstrap_period: u32,
    pub ciphertext_scale: f64,
    pub max_multiplications: u32,
}

impl Default for FheLayerConfig {
    fn default() -> Self {
        Self {
            ring_dimension: 8_192,
            bootstrap_period: 10_000,
            ciphertext_scale: 2f64.powi(40),
            max_multiplications: 1_000_000,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FheCircuitIntent {
    pub circuit_id: String,
    pub r1cs_gates: usize,
    pub depth: usize,
    pub description: String,
}

impl FheCircuitIntent {
    pub fn ckks(label: impl Into<String>, r1cs_gates: usize, depth: usize) -> Self {
        Self {
            circuit_id: label.into(),
            r1cs_gates,
            depth,
            description: "CKKS homomorphic layer".into(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum HomomorphicJob {
    Slots(Vec<f64>),
    Circuit(FheCircuitIntent, Vec<f64>),
}

pub struct FheLayer<E: FheEvaluator = MockCkksEvaluator> {
    evaluator: E,
    config: FheLayerConfig,
}

impl FheLayer<MockCkksEvaluator> {
    pub fn new(config: FheLayerConfig) -> Self {
        let evaluator = MockCkksEvaluator::new(autheo_pqcnet_5dezph::config::FheConfig {
            polynomial_degree: config.ring_dimension,
            ciphertext_scale: config.ciphertext_scale,
        });
        Self { evaluator, config }
    }
}

impl<E: FheEvaluator> FheLayer<E> {
    pub fn with_evaluator(config: FheLayerConfig, evaluator: E) -> Self {
        Self { evaluator, config }
    }

    pub fn execute(&self, job: HomomorphicJob) -> Result<FheCiphertext, FheError> {
        match job {
            HomomorphicJob::Slots(slots) => self.evaluator.encrypt(&slots),
            HomomorphicJob::Circuit(intent, slots) => {
                let ciphertext = self.evaluator.encrypt(&slots)?;
                if intent.depth as u32 > self.config.bootstrap_period {
                    return Err(FheError::new("circuit depth exceeds bootstrap budget"));
                }
                Ok(ciphertext)
            }
        }
    }

    pub fn aggregate(&self, ciphertexts: &[FheCiphertext]) -> Result<FheCiphertext, FheError> {
        self.evaluator.aggregate(ciphertexts)
    }

    pub fn bootstrap_complexity(&self) -> f64 {
        let n = self.config.ring_dimension as f64;
        (n.powi(3) * n.log2()).max(1.0)
    }

    pub fn config(&self) -> &FheLayerConfig {
        &self.config
    }
}
