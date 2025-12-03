use autheo_dw3b_overlay::{
    config::Dw3bOverlayConfig, overlay::Dw3bOverlayNode, transport::loopback_gateways,
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
