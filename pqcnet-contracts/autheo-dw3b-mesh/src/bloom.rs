use std::f64::consts::LN_2;

use bitvec::prelude::*;
use blake3::Hasher;
use serde::{Deserialize, Serialize};

/// Simple Bloom filter guard tailored for DW3B Query Mesh nodes.
#[derive(Clone, Debug)]
pub struct MeshBloomFilter {
    bits: BitVec<u8, Lsb0>,
    hash_functions: u8,
    inserted: u64,
}

impl MeshBloomFilter {
    pub fn new(capacity: u64, target_fp_rate: f64) -> Self {
        let capped_fp = target_fp_rate.clamp(1e-6, 0.25);
        let cap = capacity.max(1);
        let m = ((-1.0 * (cap as f64) * capped_fp.ln()) / (LN_2 * LN_2)).ceil() as usize;
        let k = ((m as f64 / cap as f64) * LN_2).ceil().max(1.0) as u8;
        let bits = bitvec![u8, Lsb0; 0; m.max(128)];
        Self {
            bits,
            hash_functions: k,
            inserted: 0,
        }
    }

    pub fn insert(&mut self, value: &[u8]) {
        for index in self.indexes(value) {
            self.bits.set(index, true);
        }
        self.inserted = self.inserted.saturating_add(1);
    }

    pub fn contains(&self, value: &[u8]) -> bool {
        self.indexes(value)
            .into_iter()
            .all(|idx| self.bits.get(idx).map(|bit| *bit).unwrap_or(false))
    }

    pub fn summary(&self) -> BloomMembershipSummary {
        BloomMembershipSummary {
            capacity: self.bits.len() as u64,
            inserted: self.inserted,
            fp_rate: self.false_positive_rate(),
            hash_functions: self.hash_functions,
        }
    }

    fn false_positive_rate(&self) -> f64 {
        if self.bits.is_empty() {
            return 0.0;
        }
        let m = self.bits.len() as f64;
        let k = self.hash_functions as f64;
        let n = self.inserted.max(1) as f64;
        let prob = (1.0 - (-k * n / m).exp()).powf(k);
        prob.clamp(1e-12, 0.25)
    }

    fn indexes(&self, value: &[u8]) -> Vec<usize> {
        (0..self.hash_functions)
            .map(|salt| {
                let mut hasher = Hasher::new();
                hasher.update(&salt.to_le_bytes());
                hasher.update(value);
                let digest = hasher.finalize();
                let mut buf = [0u8; 8];
                buf.copy_from_slice(&digest.as_bytes()[..8]);
                (u64::from_le_bytes(buf) as usize) % self.bits.len().max(1)
            })
            .collect()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BloomMembershipSummary {
    pub capacity: u64,
    pub inserted: u64,
    pub fp_rate: f64,
    pub hash_functions: u8,
}
