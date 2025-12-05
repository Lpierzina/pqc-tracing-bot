use autheo_privacynet::{DpQuery, PrivacyNetConfig, PrivacyNetEngine, PrivacyNetRequest};
use std::env;

#[test]
fn integrates_dp_and_ezph() {
    if !run_heavy_halo2_path() {
        eprintln!(
            "skipping heavy PrivacyNet Halo2 test (set RUN_HEAVY_PRIVACYNET=1 \
             or run `cargo test -p autheo-privacynet --features real_zk`)"
        );
        return;
    }

    let mut config = PrivacyNetConfig::default();
    config.ezph.qeh.vector_dimensions = 64;
    let mut engine = PrivacyNetEngine::new(config);

    let dp_query = DpQuery::gaussian(vec![1, 2, 3, 4], 1e-6, 2f64.powi(-40), 1.0);
    let request = PrivacyNetRequest {
        session_id: 42,
        tenant_id: "tenant-test".into(),
        label: "test-vertex".into(),
        chain_epoch: 0,
        dp_query,
        fhe_slots: vec![0.125, 0.25, 0.5, 0.75],
        parents: vec![],
        payload_bytes: 3_584,
        lamport: 1,
        contribution_score: 0.6,
        ann_similarity: 0.9,
        qrng_entropy_bits: 512,
        zk_claim: "age >= 18".into(),
        public_inputs: vec!["attr:age".into(), "bound:18".into()],
    };

    let response = engine.handle_request(request).expect("privacy pipeline");
    assert!(response.privacy_report.satisfied);
    assert!(response.dp_result.sample.noisy_vector.len() >= 1);
}

fn run_heavy_halo2_path() -> bool {
    cfg!(feature = "real_zk")
        || env_flag_enabled("RUN_HEAVY_ZK")
        || env_flag_enabled("RUN_HEAVY_PRIVACYNET")
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
