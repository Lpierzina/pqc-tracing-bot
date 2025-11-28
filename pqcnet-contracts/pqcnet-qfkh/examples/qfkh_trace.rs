use autheo_pqc_core::adapters::DemoMlKem;
use autheo_pqc_core::kem::MlKemEngine;
use autheo_pqc_core::runtime;
use pqcnet_qfkh::{QfkhConfig, QuantumForwardKeyHopper};
use std::error::Error;

mod prod_trace {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/dev_support/prod_trace.rs"
    ));
}

fn make_engine() -> MlKemEngine {
    MlKemEngine::new(Box::new(DemoMlKem::new()))
}

fn main() -> Result<(), Box<dyn Error>> {
    runtime::reset_state_for_tests();
    let prod_trace = prod_trace::ProdTrace::load();
    let config = QfkhConfig::new(
        prod_trace.config.rotation_interval_ms,
        prod_trace.config.lookahead_epochs,
    )?;
    let mut responder = QuantumForwardKeyHopper::new(make_engine(), config);
    let mut initiator = QuantumForwardKeyHopper::new(make_engine(), config);

    responder.ensure_lookahead(0)?;
    initiator.ensure_lookahead(0)?;

    for (idx, sample) in prod_trace.samples.iter().enumerate() {
        let ticket = responder.announce_epoch(sample.announce_at_ms)?;

        assert_eq!(ticket.epoch, sample.epoch, "epoch mismatch for hop {idx}");
        assert_eq!(
            ticket.window_start_ms, sample.window_start_ms,
            "window start mismatch for hop {idx}"
        );
        assert_eq!(
            ticket.window_end_ms, sample.window_end_ms,
            "window end mismatch for hop {idx}"
        );
        assert_eq!(
            ticket.key_id.0,
            prod_trace::hex_to_array(&sample.key_id_hex),
            "key id mismatch for hop {idx}"
        );
        assert_eq!(
            ticket.public_key,
            prod_trace::hex_to_vec(&sample.public_key_hex),
            "public key mismatch for hop {idx}"
        );

        let (capsule, initiator_session) =
            initiator.encapsulate_for(&ticket, sample.activate_at_ms)?;
        responder.activate_from(&capsule, sample.activate_at_ms)?;

        assert_eq!(
            capsule.ciphertext,
            prod_trace::hex_to_vec(&sample.ciphertext_hex),
            "ciphertext mismatch for hop {idx}"
        );
        assert_eq!(
            capsule.commitment,
            prod_trace::hex_to_array(&sample.commitment_hex),
            "commitment mismatch for hop {idx}"
        );
        assert_eq!(
            initiator_session.derived_key,
            prod_trace::hex_to_array(&sample.derived_key_hex),
            "derived key mismatch for hop {idx}"
        );

        println!(
            "hop {idx}: epoch={} key_id={} commitment={} ✔",
            sample.epoch,
            &sample.key_id_hex[..16],
            &sample.commitment_hex[..16]
        );
    }

    println!(
        "\nReplayed {} production hops recorded with rotation_interval={}ms",
        prod_trace.samples.len(),
        prod_trace.config.rotation_interval_ms
    );

    Ok(())
}
