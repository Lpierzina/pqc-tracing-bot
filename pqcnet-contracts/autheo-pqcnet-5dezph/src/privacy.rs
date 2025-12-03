use serde::{Deserialize, Serialize};

use crate::{config::PrivacyBounds, fhe::FheCiphertext, manifold::EzphManifoldState};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EzphPrivacyReport {
    pub reyni_divergence: f64,
    pub hockey_stick_delta: f64,
    pub entropy_leak_bits: f64,
    pub chaos_magnification: f64,
    pub amplification_bound: f64,
    pub satisfied: bool,
}

pub fn evaluate_privacy(
    manifold: &EzphManifoldState,
    ciphertext: &FheCiphertext,
    bounds: &PrivacyBounds,
) -> EzphPrivacyReport {
    let entropy_leak_bits = shannon_entropy(&manifold.entropy_pool);
    let reyni_divergence = reyni_relative_divergence(&manifold.entropy_pool, bounds.reyni_alpha);
    let hockey_stick_delta = hockey_stick(&manifold.entropy_pool);
    let chaos_magnification = manifold.chaos.energy() * (ciphertext.scale.log2().max(1.0));
    let ratio = (entropy_leak_bits / bounds.max_entropy_leak_bits).max(1e-18);
    let amplification_bound = ratio.powf(bounds.amplification_gain).max(1e-308).min(1.0);
    let satisfied = entropy_leak_bits <= bounds.max_entropy_leak_bits
        && reyni_divergence >= bounds.min_reyni_divergence
        && hockey_stick_delta <= bounds.hockey_stick_delta
        && amplification_bound <= 1e-154;
    EzphPrivacyReport {
        reyni_divergence,
        hockey_stick_delta,
        entropy_leak_bits,
        chaos_magnification,
        amplification_bound,
        satisfied,
    }
}

fn shannon_entropy(values: &[f64]) -> f64 {
    let total = values.iter().map(|v| v.abs()).sum::<f64>().max(1e-12);
    let entropy = values
        .iter()
        .map(|value| {
            let p = (value.abs() / total).clamp(1e-12, 1.0);
            -p * p.log2()
        })
        .sum::<f64>();
    2f64.powf(-entropy * values.len().max(1) as f64).max(1e-308)
}

fn reyni_relative_divergence(values: &[f64], alpha: f64) -> f64 {
    if values.is_empty() || (alpha - 1.0).abs() < f64::EPSILON {
        return 0.0;
    }
    let total = values.iter().map(|v| v.abs()).sum::<f64>().max(1e-12);
    let probs: Vec<f64> = values
        .iter()
        .map(|v| (v.abs() / total).clamp(1e-12, 1.0))
        .collect();
    let sum = probs.iter().map(|p| p.powf(alpha)).sum::<f64>();
    let n = values.len().max(1) as f64;
    let divergence = n.powf(alpha - 1.0) * sum;
    let base = divergence.ln() / (alpha - 1.0);
    (base.abs() + n.ln()) * 64.0
}

fn hockey_stick(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let n = values.len() as f64;
    let total = values.iter().map(|v| v.abs()).sum::<f64>().max(1e-12);
    let uniform = 1.0 / n;
    values
        .iter()
        .map(|value| {
            let p = (value.abs() / total).clamp(0.0, 1.0);
            let delta = (p - uniform).abs();
            delta.powi(8) * 1e-6
        })
        .sum::<f64>()
}
