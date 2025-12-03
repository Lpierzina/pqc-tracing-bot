use std::time::{SystemTime, UNIX_EPOCH};

use autheo_pqcnet_5dezph::{pipeline::EzphOutcome, privacy::EzphPrivacyReport};
use blake3::Hasher;
use serde::{Deserialize, Serialize};

use crate::{budget::BudgetClaim, chaos::ChaosSample, dp::DpSample};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CkksContextMetadata {
    pub ring_dimension: usize,
    pub ciphertext_scale: f64,
    pub operations_remaining: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrivacyEnhancedIcosuple {
    pub vertex_id: [u8; 32],
    pub spatial_coord: [u64; 8],
    pub expiry_timestamp: u64,
    pub entropy_accumulator: [u8; 32],
    pub fhe_context: CkksContextMetadata,
    pub dp_budget: [f64; 4],
    pub chaos_trajectory: [f64; 8],
    pub compressed_fhe_ct: Vec<u8>,
    pub entangled_zk_proofs: Vec<u8>,
    pub fips_signatures: Vec<u8>,
    pub privacy_report: EzphPrivacyReport,
}

impl PrivacyEnhancedIcosuple {
    pub fn assemble(
        outcome: &EzphOutcome,
        dp_sample: &DpSample,
        chaos: &ChaosSample,
        budget: &BudgetClaim,
        fhe_ops_remaining: u32,
    ) -> Self {
        let vertex_bytes = outcome.receipt.vertex_id.as_bytes();
        let mut vertex_id = [0u8; 32];
        vertex_id.copy_from_slice(vertex_bytes);
        let expiry_timestamp = current_time_ns().saturating_add(86_400_000_000_000);
        let entropy_accumulator = hash_entropy(&outcome.manifold.entropy_pool);
        let fhe_context = CkksContextMetadata {
            ring_dimension: outcome.fhe_ciphertext.slots.max(1),
            ciphertext_scale: outcome.fhe_ciphertext.scale,
            operations_remaining: fhe_ops_remaining,
        };
        let dp_budget = [
            budget.epsilon_remaining,
            budget.delta_remaining,
            dp_sample.epsilon_spent,
            dp_sample.lyapunov_amplifier,
        ];
        let chaos_trajectory = encode_chaos(chaos);
        let compressed_fhe_ct = outcome.fhe_ciphertext.digest.to_vec();
        let entangled_zk_proofs = outcome.zk_proof.proof_bytes.clone();
        let fips_signatures = outcome
            .receipt
            .pqc_signature
            .as_ref()
            .map(|sig| sig.bytes.clone())
            .unwrap_or_default();
        Self {
            vertex_id,
            spatial_coord: encode_spatial(
                &outcome.manifold.spatial,
                outcome.manifold.temporal_noise,
            ),
            expiry_timestamp,
            entropy_accumulator,
            fhe_context,
            dp_budget,
            chaos_trajectory,
            compressed_fhe_ct,
            entangled_zk_proofs,
            fips_signatures,
            privacy_report: outcome.privacy.clone(),
        }
    }
}

fn encode_spatial(spatial: &[f64; 3], temporal_noise: f64) -> [u64; 8] {
    let mut coords = [0u64; 8];
    coords[0] = (spatial[0].abs() * 1e6) as u64;
    coords[1] = (spatial[1].abs() * 1e6) as u64;
    coords[2] = (spatial[2].abs() * 1e6) as u64;
    coords[3] = (temporal_noise.abs() * 1e9) as u64;
    coords[4] = spatial.iter().map(|v| v.abs()).sum::<f64>() as u64;
    coords[5] = (spatial[0] * spatial[1]).abs() as u64;
    coords[6] = (spatial[1] * spatial[2]).abs() as u64;
    coords[7] = (spatial[2] * spatial[0]).abs() as u64;
    coords
}

fn encode_chaos(sample: &ChaosSample) -> [f64; 8] {
    [
        sample.chua_state[0],
        sample.chua_state[1],
        sample.chua_state[2],
        sample.rossler_state[0],
        sample.rossler_state[1],
        sample.rossler_state[2],
        sample.lyapunov_exponent,
        sample.entropy_seed[0] as f64,
    ]
}

fn hash_entropy(values: &[f64]) -> [u8; 32] {
    let mut hasher = Hasher::new();
    for value in values {
        hasher.update(&value.to_le_bytes());
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(hasher.finalize().as_bytes());
    out
}

fn current_time_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or_default()
}
