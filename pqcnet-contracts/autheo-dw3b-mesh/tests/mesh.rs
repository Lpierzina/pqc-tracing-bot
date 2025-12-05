use autheo_dw3b_mesh::{Dw3bMeshConfig, Dw3bMeshEngine, MeshAnonymizeRequest, QtaidProveRequest};
use once_cell::sync::Lazy;
use std::{env, sync::Mutex};

#[test]
fn dw3b_mesh_anonymize_yields_proof() {
    if !allow_dw3b_heavy_path() {
        return;
    }
    let response = with_dw3b_engine(|engine| engine.anonymize_query(MeshAnonymizeRequest::demo()))
        .expect("dw3b anonymize query");
    assert!(response.proof.route_layers >= 3);
    assert!(response.proof.metrics.k_anonymity > 0.5);
    assert!(response.route_plan.hop_count() as u32 >= response.proof.route_layers);
    assert!(!response.compressed_payload.is_empty());
}

#[test]
fn qtaid_flow_generates_tokens() {
    if !allow_dw3b_heavy_path() {
        return;
    }
    let proof = with_dw3b_engine(|engine| {
        engine.qtaid_prove(QtaidProveRequest {
            owner_did: "did:autheo:test".into(),
            trait_name: "BRCA1=negative".into(),
            genome_segment: "AGCTTAGCTA".into(),
            bits_per_snp: 4,
        })
    })
    .expect("qtaid proof");
    assert!(proof.tokens.len() >= 3);
    assert_eq!(proof.bits_per_snp, 4);
    assert!(proof.response.proof.metrics.chsh_violation > 2.8);
}

#[test]
fn entropy_beacon_has_expected_width() {
    if !allow_dw3b_heavy_path() {
        return;
    }
    let samples = with_dw3b_engine(|engine| engine.entropy_beacon(4, true));
    assert_eq!(samples.len(), 4);
    assert_eq!(samples[0].len(), 512);
}

#[test]
fn obfuscate_route_reverses_payload_and_appends_fingerprint() {
    if !allow_dw3b_heavy_path() {
        return;
    }
    let payload = b"dw3b-obfuscate-test";
    let routed = with_dw3b_engine(|engine| engine.obfuscate_route(payload, 4, 0.9))
        .expect("obfuscate route");

    let (reversed, fingerprint) = routed.split_at(payload.len());
    let mut expected = payload.to_vec();
    expected.reverse();
    assert_eq!(reversed, expected.as_slice());
    assert_eq!(fingerprint.len(), 32);
}

#[test]
fn qtaid_bits_override_updates_tokens() {
    if !allow_dw3b_heavy_path() {
        return;
    }
    let proof = with_dw3b_engine(|engine| {
        engine.qtaid_prove(QtaidProveRequest {
            owner_did: "did:autheo:traits".into(),
            trait_name: "BRCA1=negative".into(),
            genome_segment: "AGCTTAGCTA".into(),
            bits_per_snp: 6,
        })
    })
    .expect("qtaid proof");

    assert_eq!(proof.bits_per_snp, 6);
    assert!(proof
        .tokens
        .iter()
        .all(|token| token.starts_with("qtaid:6:")));
}

fn allow_dw3b_heavy_path() -> bool {
    if cfg!(feature = "real_zk")
        || env_flag_enabled("RUN_HEAVY_ZK")
        || env_flag_enabled("RUN_HEAVY_DW3B")
    {
        return true;
    }
    eprintln!(
        "skipping DW3B mesh heavy test (set RUN_HEAVY_DW3B=1 or run \
         `cargo test -p autheo-dw3b-mesh --features real_zk`)"
    );
    false
}

fn env_flag_enabled(key: &str) -> bool {
    env::var(key)
        .map(|value| is_truthy(value.trim()))
        .unwrap_or(false)
}

fn is_truthy(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

static HEAVY_ENGINE: Lazy<Mutex<Dw3bMeshEngine>> =
    Lazy::new(|| Mutex::new(Dw3bMeshEngine::new(Dw3bMeshConfig::production())));

fn with_dw3b_engine<T>(f: impl FnOnce(&mut Dw3bMeshEngine) -> T) -> T {
    let mut engine = HEAVY_ENGINE
        .lock()
        .expect("dw3b heavy engine mutex poisoned");
    f(&mut engine)
}
