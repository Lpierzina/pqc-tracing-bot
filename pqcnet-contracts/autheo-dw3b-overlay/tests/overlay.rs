use autheo_dw3b_overlay::{
    config::Dw3bOverlayConfig, overlay::Dw3bOverlayNode, parse_statement, Dw3bOverlayRpc,
    transport::loopback_gateways,
};
use serde_json::json;

#[test]
fn dw3b_anonymize_jsonrpc_roundtrip() {
    let config = Dw3bOverlayConfig::demo();
    let (gateway, _peer): (_, _) = loopback_gateways(&config.qstp).expect("loopback gateway");
    let mut node = Dw3bOverlayNode::new(config, gateway);
    let request = json!({
        "jsonrpc": "2.0",
        "id": 42,
        "method": "dw3b_anonymizeQuery",
        "params": {
            "did": "did:autheo:test",
            "attribute": "age > 21",
            "payload": "kyc-123",
            "epsilon": 1e-6,
            "delta": 2e-12,
            "route_layers": 4
        }
    });
    let response_raw = node
        .handle_jsonrpc(&request.to_string())
        .expect("jsonrpc response");
    let response: serde_json::Value = serde_json::from_str(&response_raw).unwrap();
    if response.get("error").is_some() && !response["error"].is_null() {
        panic!("overlay error: {}", response["error"]);
    }
    assert_eq!(response["id"].as_i64(), Some(42));
    let proof_id = response["result"]["proof"]["proof_id"].as_str().unwrap();
    assert!(!proof_id.is_empty());
}

#[test]
fn dw3b_qtaid_flow() {
    let config = Dw3bOverlayConfig::demo();
    let (gateway, _peer) = loopback_gateways(&config.qstp).expect("loopback");
    let mut node = Dw3bOverlayNode::new(config, gateway);
    let request = json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "dw3b_qtaidProve",
        "params": {
            "owner_did": "did:autheo:genome",
            "trait_name": "BRCA1=negative",
            "genome_segment": "AGCTTAGCTA",
            "bits_per_snp": 4
        }
    });
    let response_raw = node.handle_jsonrpc(&request.to_string()).unwrap();
    let response: serde_json::Value = serde_json::from_str(&response_raw).unwrap();
    if response.get("error").is_some() && !response["error"].is_null() {
        panic!("overlay error: {}", response["error"]);
    }
    assert_eq!(response["id"].as_i64(), Some(7));
    assert!(response["result"]["tokens"].as_array().unwrap().len() >= 3);
}

#[test]
fn grapplang_parses_anonymize_overrides() {
    let rpc = parse_statement(
        "dw3b-anonymize dw3b::attribute \
         --did did:autheo:alice \
         --payload pii-record \
         --route-layers 6 \
         --bloom-capacity 4096 \
         --fp-rate 0.02 \
         --stake-threshold 64000 \
         --lamport 777",
    )
    .expect("parse dw3b-anonymize");

    match rpc {
        Dw3bOverlayRpc::AnonymizeQuery(params) => {
            assert_eq!(params.did, "did:autheo:alice");
            assert_eq!(params.payload, "pii-record");
            assert_eq!(params.route_layers, 6);
            assert_eq!(params.bloom_capacity, Some(4096));
            assert_eq!(params.bloom_fp_rate, Some(0.02));
            assert_eq!(params.stake_threshold, Some(64000));
            assert_eq!(params.lamport_hint, Some(777));
        }
        other => panic!("unexpected rpc variant: {other:?}"),
    }
}

#[test]
fn grapplang_parses_qtaid_bits_and_owner() {
    let rpc = parse_statement(
        "qtaid-prove \"BRCA1=negative\" \
         --owner did:autheo:genome \
         --genome AGCTTAGCTA \
         --bits 6",
    )
    .expect("parse qtaid command");

    match rpc {
        Dw3bOverlayRpc::QtaidProve(params) => {
            assert_eq!(params.owner_did, "did:autheo:genome");
            assert_eq!(params.genome_segment, "AGCTTAGCTA");
            assert_eq!(params.bits_per_snp, Some(6));
        }
        other => panic!("unexpected rpc variant: {other:?}"),
    }
}

#[test]
fn dw3b_entropy_loopback_via_qstp() {
    let config = Dw3bOverlayConfig::demo();
    let (gateway, mut remote) = loopback_gateways(&config.qstp).expect("loopback");
    let mut node = Dw3bOverlayNode::new(config, gateway);
    let entropy = json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "dw3b_entropyRequest",
        "params": { "samples": 2, "dimension5": true }
    });

    remote.seal_json(&entropy).unwrap();
    let response = node
        .try_handle_qstp()
        .expect("qstp processing")
        .expect("entropy response");

    assert_eq!(response["id"].as_i64(), Some(9));
    let vrbs = response["result"]["vrbs"].as_array().expect("vrb array");
    assert_eq!(vrbs.len(), 2);
    assert!(vrbs.iter().all(|value| {
        value
            .as_str()
            .map(|hex| hex.len() == 1024)
            .unwrap_or_default()
    }));
}
