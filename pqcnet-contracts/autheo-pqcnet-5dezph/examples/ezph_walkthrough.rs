use anyhow::Result;
use autheo_pqcnet_5dezph::{DefaultEzphPipeline, EzphConfig, EzphRequest};
use autheo_pqcnet_5dqeh::{HypergraphModule, TemporalWeightModel};

fn main() -> Result<()> {
    let config = EzphConfig::default();
    let model = TemporalWeightModel::default();
    let mut module = HypergraphModule::new(config.qeh.clone(), model);
    let pipeline = DefaultEzphPipeline::new(config.clone());

    let outcome = pipeline.entangle_and_anchor(&mut module, EzphRequest::demo("validator-00"))?;
    println!(
        "Anchored vertex {} with TW {:.3} (privacy leak ≤ {:.2e})",
        outcome.receipt.vertex_id, outcome.receipt.tw_score, outcome.privacy.amplification_bound,
    );
    for projection in outcome.projections {
        println!(
            "  {:?}: ({:+.3}, {:+.3}, {:+.3}) | |v|={:.3}",
            projection.kind,
            projection.vector[0],
            projection.vector[1],
            projection.vector[2],
            projection.magnitude
        );
    }
    Ok(())
}
