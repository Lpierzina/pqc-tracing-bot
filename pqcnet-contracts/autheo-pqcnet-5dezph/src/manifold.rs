use blake3::Hasher;

use crate::{chaos::ChaosVector, config::ManifoldConfig};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DimensionKind {
    SpatialRouting,
    TemporalNoise,
    QuantumEntropy,
    ChaoticPerturbation,
    HomomorphicLayer,
}

#[derive(Clone, Debug)]
pub struct DimensionProjection {
    pub kind: DimensionKind,
    pub vector: [f64; 3],
    pub magnitude: f64,
}

#[derive(Clone, Debug)]
pub struct EzphManifoldState {
    pub spatial: [f64; 3],
    pub temporal_noise: f64,
    pub entropy_pool: Vec<f64>,
    pub chaos: ChaosVector,
    pub homomorphic_amplitude: f64,
    pub homomorphic_projection: [f64; 3],
}

impl EzphManifoldState {
    pub fn build(
        config: &ManifoldConfig,
        chaos: ChaosVector,
        seed: &[u8; 32],
        fhe_slots: &[f64],
    ) -> Self {
        let spatial = spatial_coordinate(config, &chaos);
        let entropy_pool = entropy_register(config, seed);
        let homomorphic_amplitude = fhe_slots.iter().sum::<f64>() * config.homomorphic_scale;
        let homomorphic_projection = [
            homomorphic_amplitude,
            chaos.lorenz[0] * config.homomorphic_scale,
            chaos.logistic * config.homomorphic_scale,
        ];
        let temporal_noise = chaos.gaussian_noise * config.homomorphic_scale;
        Self {
            spatial,
            temporal_noise,
            entropy_pool,
            chaos,
            homomorphic_amplitude,
            homomorphic_projection,
        }
    }

    pub fn entropy_variance(&self) -> f64 {
        if self.entropy_pool.is_empty() {
            return 0.0;
        }
        let mean = self.entropy_pool.iter().copied().sum::<f64>() / self.entropy_pool.len() as f64;
        self.entropy_pool
            .iter()
            .map(|value| (value - mean).powi(2))
            .sum::<f64>()
            / self.entropy_pool.len() as f64
    }
}

pub fn project_dimensions(
    manifold: &EzphManifoldState,
    rank: usize,
) -> Result<Vec<DimensionProjection>, usize> {
    if rank == 0 {
        return Err(rank);
    }
    let mut projections = vec![
        DimensionProjection {
            kind: DimensionKind::SpatialRouting,
            vector: manifold.spatial,
            magnitude: vector_magnitude(&manifold.spatial),
        },
        DimensionProjection {
            kind: DimensionKind::TemporalNoise,
            vector: [
                manifold.temporal_noise,
                manifold.chaos.gaussian_noise,
                manifold.chaos.logistic,
            ],
            magnitude: manifold.temporal_noise.abs() + manifold.chaos.gaussian_noise.abs(),
        },
        DimensionProjection {
            kind: DimensionKind::QuantumEntropy,
            vector: pick_entropy_axis(&manifold.entropy_pool),
            magnitude: manifold.entropy_variance().sqrt(),
        },
        DimensionProjection {
            kind: DimensionKind::ChaoticPerturbation,
            vector: manifold.chaos.chua,
            magnitude: vector_magnitude(&manifold.chaos.chua),
        },
        DimensionProjection {
            kind: DimensionKind::HomomorphicLayer,
            vector: manifold.homomorphic_projection,
            magnitude: vector_magnitude(&manifold.homomorphic_projection),
        },
    ];
    projections.sort_by(|a, b| b.magnitude.partial_cmp(&a.magnitude).unwrap());
    Ok(projections.into_iter().take(rank).collect())
}

fn spatial_coordinate(config: &ManifoldConfig, chaos: &ChaosVector) -> [f64; 3] {
    let radius = config.spatial_radius_mm.max(0.1);
    [
        chaos.lorenz[0].tanh() * radius,
        chaos.lorenz[1].tanh() * radius,
        chaos.chua[2].tanh() * radius,
    ]
}

fn entropy_register(config: &ManifoldConfig, seed: &[u8; 32]) -> Vec<f64> {
    let mut hasher = Hasher::new();
    hasher.update(seed);
    let mut reader = hasher.finalize_xof();
    let mut register = Vec::with_capacity(config.entropy_register);
    for _ in 0..config.entropy_register {
        let mut buf = [0u8; 8];
        reader.fill(&mut buf);
        let value = u64::from_le_bytes(buf) as f64 / (u64::MAX as f64);
        register.push(value);
    }
    register
}

fn pick_entropy_axis(values: &[f64]) -> [f64; 3] {
    let mut vector = [0.0; 3];
    for (idx, component) in vector.iter_mut().enumerate() {
        *component = values.get(idx).copied().unwrap_or(0.0);
    }
    vector
}

fn vector_magnitude(vector: &[f64; 3]) -> f64 {
    (vector[0].powi(2) + vector[1].powi(2) + vector[2].powi(2)).sqrt()
}
