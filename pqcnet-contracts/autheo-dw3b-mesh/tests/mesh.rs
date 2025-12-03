use autheo_dw3b_mesh::{Dw3bMeshConfig, Dw3bMeshEngine, MeshAnonymizeRequest, QtaidProveRequest};

#[test]
fn dw3b_mesh_anonymize_yields_proof() {
    let mut engine = Dw3bMeshEngine::new(Dw3bMeshConfig::production());
    let response = engine
        .anonymize_query(MeshAnonymizeRequest::demo())
        .expect("dw3b anonymize query");
    assert!(response.proof.route_layers >= 3);
    assert!(response.proof.metrics.k_anonymity > 0.5);
    assert!(response.route_plan.hop_count() as u32 >= response.proof.route_layers);
    assert!(!response.compressed_payload.is_empty());
}

#[test]
fn qtaid_flow_generates_tokens() {
    let mut engine = Dw3bMeshEngine::new(Dw3bMeshConfig::production());
    let proof = engine
        .qtaid_prove(QtaidProveRequest {
            owner_did: "did:autheo:test".into(),
            trait_name: "BRCA1=negative".into(),
            genome_segment: "AGCTTAGCTA".into(),
            bits_per_snp: 4,
        })
        .expect("qtaid proof");
    assert!(proof.tokens.len() >= 3);
    assert_eq!(proof.bits_per_snp, 4);
    assert!(proof.response.proof.metrics.chsh_violation > 2.8);
}

#[test]
fn entropy_beacon_has_expected_width() {
    let mut engine = Dw3bMeshEngine::new(Dw3bMeshConfig::production());
    let samples = engine.entropy_beacon(4, true);
    assert_eq!(samples.len(), 4);
    assert_eq!(samples[0].len(), 512);
}

#[test]
fn obfuscate_route_reverses_payload_and_appends_fingerprint() {
    let mut engine = Dw3bMeshEngine::new(Dw3bMeshConfig::production());
    let payload = b"dw3b-obfuscate-test";
    let routed = engine
        .obfuscate_route(payload, 4, 0.9)
        .expect("obfuscate route");

    let (reversed, fingerprint) = routed.split_at(payload.len());
    let mut expected = payload.to_vec();
    expected.reverse();
    assert_eq!(reversed, expected.as_slice());
    assert_eq!(fingerprint.len(), 32);
}

#[test]
fn qtaid_bits_override_updates_tokens() {
    let mut engine = Dw3bMeshEngine::new(Dw3bMeshConfig::production());
    let proof = engine
        .qtaid_prove(QtaidProveRequest {
            owner_did: "did:autheo:traits".into(),
            trait_name: "BRCA1=negative".into(),
            genome_segment: "AGCTTAGCTA".into(),
            bits_per_snp: 6,
        })
        .expect("qtaid proof");

    assert_eq!(proof.bits_per_snp, 6);
    assert!(proof
        .tokens
        .iter()
        .all(|token| token.starts_with("qtaid:6:")));
}
