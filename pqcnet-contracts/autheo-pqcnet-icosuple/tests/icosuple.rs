use autheo_pqcnet_icosuple::{
    IcosupleNetworkConfig, IcosupleNetworkSim, TierSpecialization, TupleIntent,
};
use std::collections::HashSet;

#[test]
fn tier_catalog_includes_first_twenty_specialisations() {
    let config = IcosupleNetworkConfig::default();
    let mut sim = IcosupleNetworkSim::with_seed(99, config);
    let intents = vec![
        TupleIntent::new("did:autheo:test", "identity", 2_048).with_estimated_tps(1_000_000.0)
    ];
    let telemetry = sim.propagate_batch(&intents);
    let seen = telemetry
        .tiers
        .iter()
        .filter(|tier| tier.tier_index <= 20)
        .map(|tier| tier.specialization)
        .collect::<HashSet<_>>();
    assert!(seen.contains(&TierSpecialization::ComputeHash));
    assert!(seen.contains(&TierSpecialization::MetaverseHash));
    assert!(seen.contains(&TierSpecialization::ExtensionHash));
}

#[test]
fn simulator_extends_beyond_twenty_layers() {
    let config = IcosupleNetworkConfig::default().with_tier_count(24);
    let mut sim = IcosupleNetworkSim::with_seed(11, config);
    let intents = vec![
        TupleIntent::new("did:autheo:alpha", "iot-sensor", 1_024).with_estimated_tps(4_000_000.0),
        TupleIntent::new("did:autheo:beta", "metaverse", 2_048).with_estimated_tps(4_000_000.0),
    ];
    let telemetry = sim.propagate_batch(&intents);
    let dynamic = telemetry
        .tiers
        .iter()
        .filter(|tier| matches!(tier.specialization, TierSpecialization::Dynamic(_)))
        .count();
    assert!(
        dynamic >= 1,
        "expected at least one dynamic tier, got {}",
        dynamic
    );
}
