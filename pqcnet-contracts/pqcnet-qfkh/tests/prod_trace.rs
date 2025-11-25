use autheo_pqc_core::adapters::DemoMlKem;
use autheo_pqc_core::kem::MlKemEngine;
use autheo_pqc_core::runtime;
use pqcnet_qfkh::{QfkhConfig, QuantumForwardKeyHopper};

mod prod_trace {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/dev_support/prod_trace.rs"));
}

fn make_engine() -> MlKemEngine {
    MlKemEngine::new(Box::new(DemoMlKem::new()))
}

#[test]
fn production_trace_round_trips() {
    runtime::reset_state_for_tests();
    let trace = prod_trace::ProdTrace::load();
    let config = QfkhConfig::new(
        trace.config.rotation_interval_ms,
        trace.config.lookahead_epochs,
    )
    .expect("config");
    let mut responder = QuantumForwardKeyHopper::new(make_engine(), config);
    let mut initiator = QuantumForwardKeyHopper::new(make_engine(), config);

    responder.ensure_lookahead(0).expect("lookahead");
    initiator.ensure_lookahead(0).expect("lookahead");

    for (idx, sample) in trace.samples.iter().enumerate() {
        let ticket = responder
            .announce_epoch(sample.announce_at_ms)
            .expect("announce");
        assert_eq!(ticket.epoch, sample.epoch, "epoch mismatch {idx}");
        assert_eq!(
            ticket.window_start_ms, sample.window_start_ms,
            "window start mismatch {idx}"
        );
        assert_eq!(
            ticket.window_end_ms, sample.window_end_ms,
            "window end mismatch {idx}"
        );
        assert_eq!(
            ticket.key_id.0,
            prod_trace::hex_to_array(&sample.key_id_hex),
            "key id mismatch {idx}"
        );
        assert_eq!(
            ticket.public_key,
            prod_trace::hex_to_vec(&sample.public_key_hex),
            "public key mismatch {idx}"
        );

        let (capsule, initiator_session) = initiator
            .encapsulate_for(&ticket, sample.activate_at_ms)
            .expect("encapsulate");
        let responder_session = responder
            .activate_from(&capsule, sample.activate_at_ms)
            .expect("activate");

        assert_eq!(
            capsule.ciphertext,
            prod_trace::hex_to_vec(&sample.ciphertext_hex),
            "ciphertext mismatch {idx}"
        );
        assert_eq!(
            capsule.commitment,
            prod_trace::hex_to_array(&sample.commitment_hex),
            "commitment mismatch {idx}"
        );
        let expected_key = prod_trace::hex_to_array(&sample.derived_key_hex);
        assert_eq!(
            initiator_session.derived_key, expected_key,
            "initiator derived key mismatch {idx}"
        );
        assert_eq!(
            responder_session.derived_key, expected_key,
            "responder derived key mismatch {idx}"
        );
    }
}
