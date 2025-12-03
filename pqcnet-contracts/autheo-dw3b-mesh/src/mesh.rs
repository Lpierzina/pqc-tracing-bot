use autheo_privacynet::pipeline::PrivacyNetResponse;
use blake3::Hasher;
use serde::{Deserialize, Serialize};

use crate::{
    bloom::{BloomMembershipSummary, MeshBloomFilter},
    chaos::ChaosTrajectory,
    config::MeshNodeWeights,
    entropy::EntropySnapshot,
    noise::NoiseSummary,
};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum MeshNodeKind {
    Query,
    Mixnet,
    Stake,
    Index,
    Cdn,
    Governance,
    Key,
    Micro,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RouteHop {
    pub kind: MeshNodeKind,
    pub latency_ms: u32,
    pub entropy_score: f64,
    pub stake_commitment: [u8; 32],
    pub poisson_decoys: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MeshRoutePlan {
    pub hops: Vec<RouteHop>,
    pub bloom_summary: BloomMembershipSummary,
    pub stats: MeshRouteStats,
    pub stake_threshold: u64,
    pub poisson_lambda: f64,
}

impl MeshRoutePlan {
    pub fn hop_count(&self) -> usize {
        self.hops.len()
    }

    pub fn fingerprint(&self) -> [u8; 32] {
        let mut hasher = Hasher::new();
        for hop in &self.hops {
            hasher.update(&(hop.kind as u8).to_le_bytes());
            hasher.update(&hop.latency_ms.to_le_bytes());
            hasher.update(&hop.stake_commitment);
        }
        let mut out = [0u8; 32];
        out.copy_from_slice(hasher.finalize().as_bytes());
        out
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MeshRouteStats {
    pub total_latency_ms: u32,
    pub hop_count: u32,
    pub entropy_score: f64,
}

pub struct MeshTopology {
    weights: MeshNodeWeights,
    bloom_capacity: u64,
    bloom_fp_rate: f64,
    stake_threshold: u64,
    poisson_lambda: f64,
}

impl MeshTopology {
    pub fn new(weights: MeshNodeWeights) -> Self {
        Self {
            weights: weights.normalized(),
            bloom_capacity: 1 << 20,
            bloom_fp_rate: 0.01,
            stake_threshold: 10_000,
            poisson_lambda: 10.0,
        }
    }

    pub fn with_bloom(mut self, capacity: u64, fp_rate: f64) -> Self {
        self.bloom_capacity = capacity.max(1 << 12);
        self.bloom_fp_rate = fp_rate;
        self
    }

    pub fn with_stake(mut self, stake_threshold: u64) -> Self {
        self.stake_threshold = stake_threshold;
        self
    }

    pub fn with_lambda(mut self, lambda: f64) -> Self {
        self.poisson_lambda = lambda;
        self
    }

    pub fn bloom_filter(&self) -> MeshBloomFilter {
        MeshBloomFilter::new(self.bloom_capacity, self.bloom_fp_rate)
    }

    pub fn plan_route(&self, layers: u32, entropy_seed: [u8; 32]) -> MeshRoutePlan {
        let mut hops = Vec::with_capacity(layers as usize);
        let mut hasher = Hasher::new();
        hasher.update(&entropy_seed);
        for layer in 0..layers {
            let t = (layer as f32 / layers.max(1) as f32).clamp(0.0, 1.0);
            let kind = self.pick_kind(t);
            let mut stake = [0u8; 32];
            let mut salt = hasher.clone();
            salt.update(&layer.to_le_bytes());
            stake.copy_from_slice(salt.finalize().as_bytes());
            let latency = 15 + (layer * 7) as u32;
            let entropy_score = 0.8 + 0.02 * layer as f64;
            hops.push(RouteHop {
                kind,
                latency_ms: latency,
                entropy_score,
                stake_commitment: stake,
                poisson_decoys: ((self.poisson_lambda + layer as f64).round() as u8).max(1),
            });
        }
        let stats = MeshRouteStats {
            total_latency_ms: hops.iter().map(|h| h.latency_ms).sum(),
            hop_count: hops.len() as u32,
            entropy_score: hops.iter().map(|h| h.entropy_score).sum::<f64>()
                / hops.len().max(1) as f64,
        };
        let bloom_summary = BloomMembershipSummary {
            capacity: self.bloom_capacity,
            inserted: hops.len() as u64,
            fp_rate: self.bloom_fp_rate,
            hash_functions: 3,
        };
        MeshRoutePlan {
            hops,
            bloom_summary,
            stats,
            stake_threshold: self.stake_threshold,
            poisson_lambda: self.poisson_lambda,
        }
    }

    fn pick_kind(&self, t: f32) -> MeshNodeKind {
        let mut cursor = 0.0;
        let weights = &self.weights;
        let palette = [
            (weights.query, MeshNodeKind::Query),
            (weights.mixnet, MeshNodeKind::Mixnet),
            (weights.stake, MeshNodeKind::Stake),
            (weights.index, MeshNodeKind::Index),
            (weights.cdn, MeshNodeKind::Cdn),
            (weights.governance, MeshNodeKind::Governance),
            (weights.key_management, MeshNodeKind::Key),
            (weights.micro, MeshNodeKind::Micro),
        ];
        for (weight, kind) in palette {
            cursor += weight;
            if t <= cursor {
                return kind;
            }
        }
        MeshNodeKind::Mixnet
    }

    pub fn synthesize_proof(
        &self,
        response: &PrivacyNetResponse,
        bloom: &BloomMembershipSummary,
        noise: &NoiseSummary,
        chaos: &ChaosTrajectory,
        route: &MeshRoutePlan,
        entropy: &EntropySnapshot,
    ) -> AnonymityProof {
        let proof_id = hex::encode(response.dp_result.zk_proof_digest);
        let snark = stretch("halo2", &response.dp_result.zk_proof_digest, 200);
        let stark = stretch("stark", &response.dp_result.zk_proof_digest, 96);
        let fhe = stretch("ckks", &response.enhanced_icosuple.compressed_fhe_ct, 32);
        let bloom_hash = stretch("bloom", &bloom.fp_rate.to_le_bytes(), 128);
        let pqc_sig = stretch("pqcsig", &response.enhanced_icosuple.fips_signatures, 48);
        let stake_commitment = stretch("stake", &route.fingerprint(), 1024);
        let anonymity_metric = (1.0 - bloom.fp_rate) * (noise.gaussian_sample.abs() + 1.0)
            / (route.hops.len().max(1) as f64);
        let metrics = AnonymityMetrics {
            k_anonymity: (1.0 - bloom.fp_rate).max(0.0),
            chsh_violation: 2.8 + chaos.lyapunov_exponent / 100.0,
            entropy_score: entropy.amplification_floor,
        };
        AnonymityProof {
            proof_id,
            snark_proof: snark,
            stark_proof: stark,
            fhe_digest: fhe,
            bloom_hash,
            pqc_signature: pqc_sig,
            stake_commitment,
            anonymity_metric,
            route_layers: route.hops.len() as u32,
            metrics,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct AnonymityProof {
    pub proof_id: String,
    pub snark_proof: Vec<u8>,
    pub stark_proof: Vec<u8>,
    pub fhe_digest: Vec<u8>,
    pub bloom_hash: Vec<u8>,
    pub pqc_signature: Vec<u8>,
    pub stake_commitment: Vec<u8>,
    pub anonymity_metric: f64,
    pub route_layers: u32,
    pub metrics: AnonymityMetrics,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnonymityMetrics {
    pub k_anonymity: f64,
    pub chsh_violation: f64,
    pub entropy_score: f64,
}

fn stretch(label: &str, input: &[u8], len: usize) -> Vec<u8> {
    let mut out = vec![0u8; len];
    let mut cursor = 0;
    let mut counter = 0u32;
    while cursor < len {
        let mut hasher = Hasher::new();
        hasher.update(label.as_bytes());
        hasher.update(&counter.to_le_bytes());
        hasher.update(&len.to_le_bytes());
        hasher.update(input);
        let digest = hasher.finalize();
        let chunk = digest.as_bytes();
        let take = chunk.len().min(len - cursor);
        out[cursor..cursor + take].copy_from_slice(&chunk[..take]);
        cursor += take;
        counter = counter.wrapping_add(1);
    }
    out
}
