use autheo_pqc_core::adapters::DemoMlKem;
use autheo_pqc_core::kem::MlKemEngine;
use autheo_pqc_core::runtime;
use pqcnet_qfkh::{QfkhConfig, QuantumForwardKeyHopper};
use std::error::Error;
use std::fmt::Write;

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

fn main() -> Result<(), Box<dyn Error>> {
    runtime::reset_state_for_tests();
    let config = QfkhConfig::new(5_000, 3)?;
    let mut responder =
        QuantumForwardKeyHopper::new(MlKemEngine::new(Box::new(DemoMlKem::new())), config);
    let mut initiator =
        QuantumForwardKeyHopper::new(MlKemEngine::new(Box::new(DemoMlKem::new())), config);

    responder.ensure_lookahead(0)?;
    initiator.ensure_lookahead(0)?;

    let mut now = 2_500; // midway through epoch zero
    for hop in 0..3 {
        println!("\n=== hop {hop} ===");
        let ticket = responder.announce_epoch(now)?;
        println!(
            "epoch={} window=[{}, {}) key_id={}",
            ticket.epoch,
            ticket.window_start_ms,
            ticket.window_end_ms,
            to_hex(&ticket.key_id.0[..8])
        );

        let (capsule, initiator_session) = initiator.encapsulate_for(&ticket, now + 250)?;
        println!(
            "capsule commitment={} derived_key={}",
            to_hex(&capsule.commitment[..8]),
            to_hex(&initiator_session.derived_key[..8])
        );

        let responder_session = responder.activate_from(&capsule, now + 250)?;
        println!(
            "responder derived_key={} rotation_required?={}",
            to_hex(&responder_session.derived_key[..8]),
            responder.needs_rotation(now + config.rotation_interval_ms)
        );

        now += config.rotation_interval_ms;
    }

    Ok(())
}
