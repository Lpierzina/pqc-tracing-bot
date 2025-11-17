use crate::error::{PqcError, PqcResult};
use crate::qstp::{MeshQosClass, MeshRoutePlan, TunnelId};
use alloc::borrow::Cow;
use alloc::collections::BTreeSet;
use alloc::vec::Vec;
use core::cmp;

/// Telemetry surfaced to QACE (Quantum Adaptive Chaos Engine).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QaceMetrics {
    pub latency_ms: u32,
    pub loss_bps: u32,
    pub threat_score: u8,
    pub route_changes: u8,
    pub jitter_ms: u32,
    pub bandwidth_mbps: u32,
    pub chaos_level: u8,
}

impl QaceMetrics {
    pub const fn new(
        latency_ms: u32,
        loss_bps: u32,
        threat_score: u8,
        route_changes: u8,
        jitter_ms: u32,
        bandwidth_mbps: u32,
        chaos_level: u8,
    ) -> Self {
        Self {
            latency_ms,
            loss_bps,
            threat_score,
            route_changes,
            jitter_ms,
            bandwidth_mbps,
            chaos_level,
        }
    }
}

impl Default for QaceMetrics {
    fn default() -> Self {
        Self {
            latency_ms: 0,
            loss_bps: 0,
            threat_score: 0,
            route_changes: 0,
            jitter_ms: 0,
            bandwidth_mbps: 0,
            chaos_level: 0,
        }
    }
}

/// Ordered set of candidate paths (primary + alternates).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathSet {
    pub primary: MeshRoutePlan,
    pub alternates: Vec<MeshRoutePlan>,
}

impl PathSet {
    pub fn new(primary: MeshRoutePlan, alternates: Vec<MeshRoutePlan>) -> Self {
        Self {
            primary,
            alternates,
        }
    }

    pub fn len(&self) -> usize {
        1 + self.alternates.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn flatten(&self) -> Vec<MeshRoutePlan> {
        let mut out = Vec::with_capacity(self.len());
        out.push(self.primary.clone());
        out.extend(self.alternates.clone());
        out
    }
}

/// Immutable snapshot forwarded to engines.
#[derive(Clone, Debug)]
pub struct QaceRequest<'a> {
    pub tunnel_id: &'a TunnelId,
    pub telemetry_epoch: u64,
    pub metrics: QaceMetrics,
    pub path_set: PathSet,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QaceDecision {
    pub action: QaceAction,
    pub score: i64,
    pub rationale: Cow<'static, str>,
    pub path_set: PathSet,
    pub convergence: QaceConvergence,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QaceConvergence {
    pub generations: u32,
    pub stale_generations: u32,
    pub confidence: f32,
}

impl QaceConvergence {
    pub fn deterministic() -> Self {
        Self {
            generations: 1,
            stale_generations: 0,
            confidence: 1.0,
        }
    }
}

/// Actions supported by the adaptive controller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QaceAction {
    Maintain,
    Rekey,
    Reroute(MeshRoutePlan),
}

/// Hook invoked when telemetry indicates a threat or path degradation.
pub trait QaceEngine {
    fn evaluate(&mut self, request: QaceRequest) -> PqcResult<QaceDecision>;
}

/// Deterministic fallback controller (used in WASM / tests).
#[derive(Clone, Debug, Default)]
pub struct SimpleQace {
    last_score: i64,
}

impl SimpleQace {
    fn score(metrics: &QaceMetrics) -> i64 {
        let latency_pen = metrics.latency_ms.min(10_000) as i64 * 12;
        let loss_pen = (metrics.loss_bps / 500) as i64 * 9;
        let threat_pen = metrics.threat_score as i64 * 20;
        let jitter_pen = metrics.jitter_ms as i64 * 8;
        let chaos_pen = metrics.chaos_level as i64 * 15;
        120_000 - latency_pen - loss_pen - threat_pen - jitter_pen - chaos_pen
    }
}

impl QaceEngine for SimpleQace {
    fn evaluate(&mut self, request: QaceRequest) -> PqcResult<QaceDecision> {
        if request.path_set.is_empty() {
            return Err(PqcError::InvalidInput("qace path set empty"));
        }
        let mut decision = QaceDecision {
            action: QaceAction::Maintain,
            score: Self::score(&request.metrics),
            rationale: Cow::Borrowed("heuristic-stable"),
            path_set: request.path_set.clone(),
            convergence: QaceConvergence::deterministic(),
        };

        if request.metrics.threat_score >= 80 && !request.path_set.alternates.is_empty() {
            let mut reordered = request.path_set.clone();
            let promoted = reordered.alternates.remove(0);
            reordered.alternates.insert(0, reordered.primary.clone());
            reordered.primary = promoted.clone();
            decision.action = QaceAction::Reroute(promoted);
            decision.rationale = Cow::Borrowed("threat-score-reroute");
            decision.path_set = reordered;
        } else if request.metrics.loss_bps >= 5_000 {
            decision.action = QaceAction::Rekey;
            decision.rationale = Cow::Borrowed("high-loss-rekey");
        }

        self.last_score = decision.score;
        Ok(decision)
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod ga {
    use super::*;
    use alloc::string::String;
    use genetic_algorithm::strategy::evolve::prelude::*;

    #[derive(Clone, Debug)]
    pub struct QaceGaConfig {
        pub population_size: usize,
        pub max_generations: usize,
        pub max_stale_generations: usize,
        pub mutation_probability: f32,
        pub crossover_rate: f32,
        pub selection_rate: f32,
        pub replacement_rate: f32,
        pub elitism_rate: f32,
        pub tournament_size: usize,
        pub duplicate_penalty: i32,
        pub threat_reroute_score: u8,
        pub loss_rekey_threshold: u32,
        pub rng_seed: Option<u64>,
    }

    impl Default for QaceGaConfig {
        fn default() -> Self {
            Self {
                population_size: 48,
                max_generations: 64,
                max_stale_generations: 16,
                mutation_probability: 0.18,
                crossover_rate: 0.75,
                selection_rate: 0.6,
                replacement_rate: 0.65,
                elitism_rate: 0.04,
                tournament_size: 7,
                duplicate_penalty: 900,
                threat_reroute_score: 70,
                loss_rekey_threshold: 8_000,
                rng_seed: None,
            }
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct QaceWeights {
        pub latency: u32,
        pub loss: u32,
        pub threat: u32,
        pub jitter: u32,
        pub congestion: u32,
        pub hop_penalty: u32,
        pub qos_gain: u32,
        pub freshness: u32,
        pub stability: u32,
    }

    impl Default for QaceWeights {
        fn default() -> Self {
            Self {
                latency: 11,
                loss: 7,
                threat: 19,
                jitter: 5,
                congestion: 3,
                hop_penalty: 13,
                qos_gain: 17,
                freshness: 2,
                stability: 4,
            }
        }
    }

    #[derive(Clone, Debug)]
    pub struct GaQace {
        config: QaceGaConfig,
        weights: QaceWeights,
    }

    impl Default for GaQace {
        fn default() -> Self {
            Self::new(QaceGaConfig::default(), QaceWeights::default())
        }
    }

    impl GaQace {
        pub fn new(config: QaceGaConfig, weights: QaceWeights) -> Self {
            Self { config, weights }
        }

        fn short_circuit(&self, request: QaceRequest) -> PqcResult<QaceDecision> {
            let mut simple = SimpleQace::default();
            simple.evaluate(request)
        }

        fn select_action(
            &self,
            request: &QaceRequest,
            new_primary: &MeshRoutePlan,
            reroute_occurred: bool,
        ) -> (QaceAction, &'static str) {
            if reroute_occurred {
                return (
                    QaceAction::Reroute(new_primary.clone()),
                    if request.metrics.threat_score >= self.config.threat_reroute_score {
                        "ga-threat-reroute"
                    } else {
                        "ga-optimization"
                    },
                );
            }
            if request.metrics.loss_bps >= self.config.loss_rekey_threshold {
                return (QaceAction::Rekey, "ga-rekey");
            }
            if request.metrics.threat_score >= self.config.threat_reroute_score {
                return (QaceAction::Rekey, "ga-threat-rekey");
            }
            (QaceAction::Maintain, "ga-stable")
        }
    }

    impl QaceEngine for GaQace {
        fn evaluate(&mut self, request: QaceRequest) -> PqcResult<QaceDecision> {
            let candidate_routes = request.path_set.flatten();
            if candidate_routes.is_empty() {
                return Err(PqcError::InvalidInput("qace candidates missing"));
            }
            if candidate_routes.len() == 1 {
                return self.short_circuit(request);
            }

            let allele_list: Vec<usize> = (0..candidate_routes.len()).collect();
            let genotype = ListGenotype::builder()
                .with_genes_size(candidate_routes.len())
                .with_allele_list(allele_list)
                .with_genes_hashing(true)
                .with_chromosome_recycling(true)
                .build()
                .map_err(|_| PqcError::IntegrationError("qace genotype build failed".into()))?;

            let attributes = candidate_routes
                .iter()
                .map(RouteAttributes::from_plan)
                .collect::<Vec<_>>();

            let fitness = RouteFitness {
                metrics: request.metrics,
                weights: self.weights,
                attributes,
                duplicate_penalty: self.config.duplicate_penalty as i64,
            };

            let evolve = Evolve::builder()
                .with_genotype(genotype)
                .with_select(SelectTournament::new(
                    self.config.replacement_rate,
                    self.config.elitism_rate,
                    cmp::max(2, self.config.tournament_size),
                ))
                .with_crossover(CrossoverUniform::new(
                    self.config.selection_rate,
                    self.config.crossover_rate,
                ))
                .with_mutate(MutateSingleGene::new(self.config.mutation_probability))
                .with_fitness(fitness)
                .with_fitness_ordering(FitnessOrdering::Maximize)
                .with_target_population_size(self.config.population_size.max(8))
                .with_max_generations(self.config.max_generations.max(4))
                .with_max_stale_generations(self.config.max_stale_generations.max(4))
                .with_rng_seed_from_u64_option(self.config.rng_seed)
                .call()
                .map_err(|_| PqcError::IntegrationError("ga evolve failed".into()))?;

            let (best_genes, best_score) = evolve
                .best_genes_and_fitness_score()
                .ok_or_else(|| PqcError::IntegrationError("ga failed to converge".into()))?;

            let mut ordered = Vec::with_capacity(candidate_routes.len());
            let mut seen = BTreeSet::new();
            for allele in &best_genes {
                let idx = (*allele % candidate_routes.len()).min(candidate_routes.len() - 1);
                if seen.insert(idx) {
                    ordered.push(idx);
                }
            }
            for idx in 0..candidate_routes.len() {
                if seen.insert(idx) {
                    ordered.push(idx);
                }
            }

            let mut reordered_alts = Vec::with_capacity(candidate_routes.len().saturating_sub(1));
            for idx in ordered.iter().skip(1) {
                reordered_alts.push(candidate_routes[*idx].clone());
            }
            let new_primary = candidate_routes[ordered[0]].clone();
            let mut path_set = PathSet::new(new_primary.clone(), reordered_alts);
            if path_set.primary.topic.is_empty() {
                path_set.primary = request.path_set.primary.clone();
            }

            let reroute_occurred =
                path_set.primary.route_hash() != request.path_set.primary.route_hash();
            let (action, rationale) =
                self.select_action(&request, &path_set.primary, reroute_occurred);

            let convergence = {
                let generations = evolve.state.current_generation as u32;
                let stale = evolve.state.stale_generations as u32;
                let confidence = if generations == 0 {
                    1.0
                } else {
                    let ratio = 1.0 - (stale.min(generations) as f32 / generations as f32);
                    ratio.max(0.0)
                };
                QaceConvergence {
                    generations: generations.max(1),
                    stale_generations: stale,
                    confidence,
                }
            };

            Ok(QaceDecision {
                action,
                score: best_score as i64,
                rationale: Cow::Owned(String::from(rationale)),
                path_set,
                convergence,
            })
        }
    }

    #[derive(Clone, Debug)]
    struct RouteAttributes {
        hop_count: u32,
        qos_bias: i64,
        freshness: i64,
    }

    impl RouteAttributes {
        fn from_plan(plan: &MeshRoutePlan) -> Self {
            let qos_bias = match plan.qos {
                MeshQosClass::LowLatency => 5,
                MeshQosClass::Control => 3,
                MeshQosClass::Gossip => 1,
            };
            Self {
                hop_count: plan.hops.len() as u32,
                qos_bias,
                freshness: plan.epoch as i64,
            }
        }

        fn base_score(&self, weights: &QaceWeights) -> i64 {
            let qos_component = self.qos_bias * weights.qos_gain as i64;
            let hop_penalty = -(self.hop_count as i64) * weights.hop_penalty as i64;
            let freshness = self.freshness * weights.freshness as i64;
            qos_component + hop_penalty + freshness
        }
    }

    #[derive(Clone, Debug)]
    struct RouteFitness {
        metrics: QaceMetrics,
        weights: QaceWeights,
        attributes: Vec<RouteAttributes>,
        duplicate_penalty: i64,
    }

    impl RouteFitness {
        fn slot_multiplier(&self, slot: usize) -> i64 {
            match slot {
                0 => 3 * self.weights.stability as i64,
                1 => 2 * self.weights.stability as i64,
                _ => self.weights.stability as i64,
            }
        }

        fn metric_penalty(&self, slot: usize) -> i64 {
            let slot_factor = (slot as i64 + 1).max(1);
            let latency = self.metrics.latency_ms as i64 * self.weights.latency as i64;
            let loss = (self.metrics.loss_bps / 500) as i64 * self.weights.loss as i64;
            let threat = self.metrics.threat_score as i64 * self.weights.threat as i64;
            let jitter = self.metrics.jitter_ms as i64 * self.weights.jitter as i64;
            let congestion = self.metrics.bandwidth_mbps as i64 * self.weights.congestion as i64;
            (latency + loss + threat + jitter + congestion) * slot_factor
        }
    }

    impl Fitness for RouteFitness {
        type Genotype = ListGenotype;

        fn calculate_for_chromosome(
            &mut self,
            chromosome: &FitnessChromosome<Self>,
            _genotype: &FitnessGenotype<Self>,
        ) -> Option<FitnessValue> {
            if self.attributes.is_empty() {
                return None;
            }
            let mut seen = BTreeSet::new();
            let mut score: i64 = 0;
            for (slot, allele) in chromosome.genes.iter().enumerate() {
                let idx = (*allele % self.attributes.len()).min(self.attributes.len() - 1);
                let attr = &self.attributes[idx];
                let base = attr.base_score(&self.weights);
                let slot_mul = self.slot_multiplier(slot);
                score += base * slot_mul;
                score -= self.metric_penalty(slot);
                if !seen.insert(idx) {
                    score -= self.duplicate_penalty;
                }
            }
            let bounded = score.clamp(FitnessValue::MIN as i64 + 1, FitnessValue::MAX as i64 - 1);
            Some(bounded as FitnessValue)
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use ga::{GaQace, QaceGaConfig, QaceWeights};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qstp::{MeshPeerId, MeshQosClass, MeshRoutePlan, TunnelId};

    fn demo_route(topic: &str, qos: MeshQosClass, epoch: u64, hops: usize) -> MeshRoutePlan {
        MeshRoutePlan {
            topic: topic.into(),
            hops: (0..hops)
                .map(|i| MeshPeerId::derive(&format!("{topic}-hop-{i}")))
                .collect(),
            qos,
            epoch,
        }
    }

    #[test]
    fn simple_qace_triggers_reroute_on_threat() {
        let mut engine = SimpleQace::default();
        let path_set = PathSet::new(
            demo_route("primary", MeshQosClass::LowLatency, 1, 2),
            vec![demo_route("failsafe", MeshQosClass::Control, 2, 1)],
        );
        let decision = engine
            .evaluate(QaceRequest {
                tunnel_id: &TunnelId([0u8; 16]),
                telemetry_epoch: 2,
                metrics: QaceMetrics {
                    latency_ms: 3,
                    loss_bps: 100,
                    threat_score: 95,
                    route_changes: 0,
                    jitter_ms: 1,
                    bandwidth_mbps: 12,
                    chaos_level: 5,
                },
                path_set,
            })
            .expect("simple qace");
        assert!(matches!(decision.action, QaceAction::Reroute(_)));
        assert_eq!(decision.path_set.primary.topic, "failsafe");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn ga_qace_prefers_low_latency_route() {
        let mut engine = crate::qace::GaQace::new(
            crate::qace::QaceGaConfig {
                rng_seed: Some(42),
                ..Default::default()
            },
            crate::qace::QaceWeights::default(),
        );
        let path_set = PathSet::new(
            demo_route("high-hop", MeshQosClass::Gossip, 1, 4),
            vec![
                demo_route("low-hop", MeshQosClass::LowLatency, 2, 1),
                demo_route("control", MeshQosClass::Control, 3, 2),
            ],
        );
        let decision = engine
            .evaluate(QaceRequest {
                tunnel_id: &TunnelId([1u8; 16]),
                telemetry_epoch: 2,
                metrics: QaceMetrics {
                    latency_ms: 7,
                    loss_bps: 8_500,
                    threat_score: 40,
                    route_changes: 2,
                    jitter_ms: 5,
                    bandwidth_mbps: 30,
                    chaos_level: 2,
                },
                path_set,
            })
            .expect("ga decision");
        assert_eq!(decision.path_set.primary.topic, "low-hop");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn ga_qace_reports_convergence_under_chaos() {
        let mut engine = crate::qace::GaQace::new(
            crate::qace::QaceGaConfig {
                rng_seed: Some(17),
                ..Default::default()
            },
            crate::qace::QaceWeights::default(),
        );
        let path_set = PathSet::new(
            demo_route("chaos-main", MeshQosClass::LowLatency, 10, 2),
            vec![demo_route("chaos-alt", MeshQosClass::Control, 11, 1)],
        );
        let decision = engine
            .evaluate(QaceRequest {
                tunnel_id: &TunnelId([3u8; 16]),
                telemetry_epoch: 11,
                metrics: QaceMetrics {
                    latency_ms: 12,
                    loss_bps: 15_000,
                    threat_score: 88,
                    route_changes: 4,
                    chaos_level: 12,
                    jitter_ms: 9,
                    bandwidth_mbps: 55,
                },
                path_set,
            })
            .expect("ga decision chaos");
        assert!(
            decision.convergence.generations >= 1,
            "expected generations recorded"
        );
        assert!(
            (0.0..=1.0).contains(&decision.convergence.confidence),
            "confidence must be normalized"
        );
    }
}
