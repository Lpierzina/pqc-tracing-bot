use autheo_pqcnet_5dezph::{DefaultEzphPipeline, EzphConfig, EzphRequest};
use autheo_pqcnet_5dqeh::{HypergraphModule, TemporalWeightModel};
use std::env;

#[test]
fn pipeline_anchors_vertex_and_checks_privacy() {
    if !allow_ezph_heavy_path() {
        return;
    }

    let config = EzphConfig::default();
    let mut module = HypergraphModule::new(config.qeh.clone(), TemporalWeightModel::default());
    let pipeline = DefaultEzphPipeline::new(config.clone());

    let outcome = pipeline
        .entangle_and_anchor(&mut module, EzphRequest::demo("validator-test"))
        .expect("ezph anchor should succeed");

    assert!(outcome.privacy.satisfied);
    assert_eq!(module.storage_layout().total_vertices(), 1);
}

fn allow_ezph_heavy_path() -> bool {
    if cfg!(feature = "real_zk")
        || env_flag_enabled("RUN_HEAVY_ZK")
        || env_flag_enabled("RUN_HEAVY_EZPH")
    {
        return true;
    }

    eprintln!(
        "skipping EZPH pipeline heavy test (set RUN_HEAVY_EZPH=1 or run \
         `cargo test -p autheo-pqcnet-5dezph --features real_zk`)"
    );
    false
}

fn env_flag_enabled(key: &str) -> bool {
    env::var(key).map(|value| is_truthy(value.trim())).unwrap_or(false)
}

fn is_truthy(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}
