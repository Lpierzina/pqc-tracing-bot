use blake3::Hasher;
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};

use crate::config::QuantumEntropyConfig;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntropySnapshot {
    pub samples_generated: u64,
    pub amplification_floor: f64,
    pub last_seed: [u8; 32],
    pub beacon_url: String,
}

pub struct QuantumEntropyPool {
    rng: ChaCha20Rng,
    config: QuantumEntropyConfig,
    generated: u64,
    last_seed: [u8; 32],
}

impl QuantumEntropyPool {
    pub fn new(config: QuantumEntropyConfig) -> Self {
        let mut hasher = Hasher::new();
        hasher.update(&config.dimension.to_le_bytes());
        hasher.update(&config.vrb_size_bits.to_le_bytes());
        let mut seed = [0u8; 32];
        seed.copy_from_slice(hasher.finalize().as_bytes());
        Self {
            rng: ChaCha20Rng::from_seed(seed),
            config,
            generated: 0,
            last_seed: seed,
        }
    }

    pub fn next_seed(&mut self, context: &[u8]) -> [u8; 32] {
        let mut raw = [0u8; 32];
        self.rng.fill_bytes(&mut raw);
        let mut hasher = Hasher::new();
        hasher.update(&raw);
        hasher.update(context);
        hasher.update(&self.generated.to_le_bytes());
        let mut seed = [0u8; 32];
        seed.copy_from_slice(hasher.finalize().as_bytes());
        self.last_seed = seed;
        seed
    }

    pub fn vrbs(&mut self, samples: u32) -> Vec<[u8; 512]> {
        (0..samples)
            .map(|index| {
                let mut block = [0u8; 512];
                self.rng.fill_bytes(&mut block);
                // encode index + dimension so auditors can replay the beacon
                block[0] ^= self.config.dimension;
                block[1] ^= (index & 0xFF) as u8;
                block[2] ^= (index >> 8) as u8;
                self.generated += 1;
                block
            })
            .collect()
    }

    pub fn snapshot(&self) -> EntropySnapshot {
        EntropySnapshot {
            samples_generated: self.generated,
            amplification_floor: self.config.amplification_target,
            last_seed: self.last_seed,
            beacon_url: self.config.beacon_url.clone(),
        }
    }

    pub fn config(&self) -> &QuantumEntropyConfig {
        &self.config
    }
}
