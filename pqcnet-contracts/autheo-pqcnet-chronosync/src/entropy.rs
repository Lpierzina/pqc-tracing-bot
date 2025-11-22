#![cfg(feature = "sim")]

use pqcnet_entropy::{EntropySource, SimEntropySource};

const ENTROPY_PANIC: &str =
    "chronosync entropy source failed - ensure autheo_host_entropy is wired via pqcnet-entropy";

pub struct ChronosyncEntropyRng {
    source: SimEntropySource,
}

impl ChronosyncEntropyRng {
    pub fn with_seed(seed: u64) -> Self {
        Self {
            source: SimEntropySource::with_seed(seed),
        }
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut bytes = [0u8; 8];
        self.source
            .try_fill_bytes(&mut bytes)
            .expect(ENTROPY_PANIC);
        u64::from_le_bytes(bytes)
    }

    pub fn unit(&mut self) -> f64 {
        const SCALE: f64 = (u64::MAX as f64) + 1.0;
        (self.next_u64() as f64) / SCALE
    }

    pub fn sample_index(&mut self, upper: usize) -> usize {
        if upper == 0 {
            return 0;
        }
        if upper == 1 {
            return 0;
        }
        (self.next_u64() as usize) % upper
    }

    pub fn sample_weighted_index(&mut self, weights: &[f64]) -> usize {
        if weights.is_empty() {
            return 0;
        }
        let total: f64 = weights.iter().map(|w| w.max(0.0)).sum();
        if total <= f64::EPSILON {
            return self.sample_index(weights.len());
        }
        let mut draw = self.unit() * total;
        for (idx, weight) in weights.iter().enumerate() {
            let capped = weight.max(0.0);
            if draw <= capped {
                return idx;
            }
            draw -= capped;
        }
        weights.len().saturating_sub(1)
    }

    pub fn range_f64(&mut self, min: f64, max: f64) -> f64 {
        if (max - min).abs() <= f64::EPSILON {
            return min;
        }
        min + self.unit() * (max - min)
    }
}
