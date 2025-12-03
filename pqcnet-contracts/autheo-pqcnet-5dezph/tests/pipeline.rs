use autheo_pqcnet_5dezph::{DefaultEzphPipeline, EzphConfig, EzphRequest};
use autheo_pqcnet_5dqeh::{HypergraphModule, TemporalWeightModel};

#[test]
fn pipeline_anchors_vertex_and_checks_privacy() {
    let config = EzphConfig::default();
    let mut module = HypergraphModule::new(config.qeh.clone(), TemporalWeightModel::default());
    let pipeline = DefaultEzphPipeline::new(config.clone());

    let outcome = pipeline
        .entangle_and_anchor(&mut module, EzphRequest::demo("validator-test"))
        .expect("ezph anchor should succeed");

    assert!(outcome.privacy.satisfied);
    assert_eq!(module.storage_layout().total_vertices(), 1);
}
