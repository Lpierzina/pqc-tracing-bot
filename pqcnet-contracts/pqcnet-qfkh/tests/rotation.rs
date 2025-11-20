use autheo_pqc_core::adapters::DemoMlKem;
use autheo_pqc_core::kem::MlKemEngine;
use autheo_pqc_core::runtime;
use pqcnet_qfkh::{QfkhConfig, QuantumForwardKeyHopper};

fn make_engine() -> MlKemEngine {
    MlKemEngine::new(Box::new(DemoMlKem::new()))
}

#[test]
fn consecutive_hops_rotate_keys() {
    runtime::reset_state_for_tests();
    let config = QfkhConfig::new(3_000, 2).expect("config");
    let mut responder = QuantumForwardKeyHopper::new(make_engine(), config);
    let mut initiator = QuantumForwardKeyHopper::new(make_engine(), config);

    responder.ensure_lookahead(0).expect("lookahead");
    initiator.ensure_lookahead(0).expect("lookahead");

    let mut previous = None;
    for hop in 0..3 {
        let now = hop as u64 * config.rotation_interval_ms + 1_000;
        let ticket = responder.announce_epoch(now).expect("announce");
        let (capsule, initiator_key) = initiator
            .encapsulate_for(&ticket, now)
            .expect("encapsulate");
        let responder_key = responder.activate_from(&capsule, now).expect("activate");
        assert_eq!(initiator_key.derived_key, responder_key.derived_key);
        if let Some(prev) = previous.replace(initiator_key.derived_key) {
            assert_ne!(prev, initiator_key.derived_key, "hop {hop} did not rotate");
        }
    }
}
