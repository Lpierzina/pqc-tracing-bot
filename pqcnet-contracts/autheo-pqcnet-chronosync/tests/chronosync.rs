use autheo_pqcnet_chronosync::{
    ChronosyncConfig, ChronosyncNodeProfile, ChronosyncSim, MAX_PARENT_REFERENCES,
};

fn demo_nodes() -> Vec<ChronosyncNodeProfile> {
    vec![
        ChronosyncNodeProfile::new("node-a")
            .with_longevity_hours(24 * 480)
            .with_proof_of_burn(1.0)
            .with_zkp_validations(2_000),
        ChronosyncNodeProfile::new("node-b")
            .with_longevity_hours(24 * 360)
            .with_proof_of_burn(0.8)
            .with_zkp_validations(1_500),
        ChronosyncNodeProfile::new("node-c")
            .with_longevity_hours(24 * 240)
            .with_proof_of_burn(0.6)
            .with_zkp_validations(900),
        ChronosyncNodeProfile::new("node-d")
            .with_longevity_hours(24 * 120)
            .with_proof_of_burn(0.4)
            .with_zkp_validations(700),
        ChronosyncNodeProfile::new("node-e")
            .with_longevity_hours(24 * 60)
            .with_proof_of_burn(0.2)
            .with_zkp_validations(400),
        ChronosyncNodeProfile::new("node-f")
            .with_longevity_hours(24 * 30)
            .with_proof_of_burn(0.1)
            .with_zkp_validations(120)
            .with_suspicion_events(1),
    ]
}

#[test]
fn pools_match_configured_dimensions() {
    let mut config = ChronosyncConfig::default();
    config.verification_pools = 4;
    config.subpool_size = 3;
    let mut sim = ChronosyncSim::with_seed(7, config.clone());

    let report = sim.drive_epoch(&demo_nodes(), 900_000_000);
    assert_eq!(report.pools.len(), config.verification_pools);
    assert!(report
        .pools
        .iter()
        .all(|pool| pool.selections.len() == config.subpool_size));
}

#[test]
fn dag_layers_and_parent_caps_hold() {
    let mut config = ChronosyncConfig::default();
    config.layers = 5;
    config.max_parents = 6;
    let mut sim = ChronosyncSim::with_seed(99, config.clone());

    let report = sim.drive_epoch(&demo_nodes(), 1_200_000_000);
    assert_eq!(report.dag_witness.nodes.len(), config.layers as usize);
    for node in &report.dag_witness.nodes {
        assert!(node.parents.len() <= config.max_parents);
        assert!(node.parents.len() <= MAX_PARENT_REFERENCES);
    }
}

#[test]
fn fairness_gini_stays_under_threshold() {
    let config = ChronosyncConfig::default();
    let mut sim = ChronosyncSim::with_seed(1337, config);
    let report = sim.drive_epoch(&demo_nodes(), 2_000_000_000);
    assert!(report.fairness_gini < 0.5);
}
