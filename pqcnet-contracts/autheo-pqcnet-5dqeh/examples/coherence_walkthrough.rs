use autheo_pqcnet_5dqeh::{
    FiveDqehSim, HypergraphModule, QehConfig, SimulationIntent, TemporalWeightModel, VertexId,
};
 use rand::{rngs::StdRng, SeedableRng};
 
 fn main() {
     let config = QehConfig::default();
     let weight_model = TemporalWeightModel::default();
    let mut module = HypergraphModule::new(config.clone(), weight_model);
     let mut sim = FiveDqehSim::with_seed(7, config, weight_model);
 
     let mut parent_rng = StdRng::seed_from_u64(9001);
     let parent = VertexId::random(&mut parent_rng);
 
     let intents = vec![
         SimulationIntent::entangle("genesis-icosuple", vec![], 2_048, 1, 0.42, 0.88, 256),
         SimulationIntent::entangle(
             "laser-spine",
             vec![parent],
             3_600,
             86_400_000,
             0.73,
             0.71,
             384,
         ),
     ];
 
    let report = sim.drive_epoch(&mut module, intents);
     println!(
        "epoch {}: {} accepted, {} archived (coherence {:.2}) hot={} crystal={}",
        report.epoch_index,
        report.accepted_vertices,
        report.crystalline_archives,
        report.coherence_index,
        report.storage_layout.hot_vertices,
        report.storage_layout.crystalline_vertices
     );
 
     for path in report.laser_paths.iter().take(3) {
         println!(
             "channel {} → {:.0} Gbps @ {:.2} ps (QKD={})",
             path.channel_id, path.throughput_gbps, path.latency_ps, path.qkd_active
         );
     }
 }
