use autheo_privacynet::{DpQuery, PrivacyNetConfig, PrivacyNetEngine, PrivacyNetRequest};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = PrivacyNetConfig::default();
    config.ezph.qeh.vector_dimensions = 64;
    let mut engine = PrivacyNetEngine::new(config);

    let dp_query = DpQuery::gaussian(vec![42, 7, 13], 1e-6, 2f64.powi(-40), 1.0);
    let request = PrivacyNetRequest {
        session_id: 1,
        tenant_id: "tenant-alpha".into(),
        label: "privacynet-demo".into(),
        chain_epoch: 0,
        dp_query,
        fhe_slots: vec![0.125, 0.25, 0.5, 0.75],
        parents: vec![],
        payload_bytes: 4_096,
        lamport: 1,
        contribution_score: 0.62,
        ann_similarity: 0.91,
        qrng_entropy_bits: 512,
        zk_claim: "age >= 18".into(),
        public_inputs: vec!["attr:age".into(), "bound:18".into()],
    };

    let response = engine.handle_request(request)?;
    println!(
        "Anchored vertex {} with TW privacy satisfied: {}",
        hexify(&response.enhanced_icosuple.vertex_id),
        response.privacy_report.satisfied
    );
    println!(
        "DP epsilon remaining: {:.2e}",
        response.dp_result.budget_claim.epsilon_remaining
    );
    println!(
        "Chaos trajectory lyapunov: {:.4}",
        response.chaos_sample.lyapunov_exponent
    );
    Ok(())
}

fn hexify(id: &[u8; 32]) -> String {
    id.iter().map(|byte| format!("{byte:02x}")).collect()
}
