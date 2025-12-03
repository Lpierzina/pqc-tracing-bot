use autheo_pqcnet_5dqeh::{Icosuple, QehConfig};
use autheo_privacynet_network_overlay::{
    grapplang::parse_statement, overlay::OverlayNode, rpc::OverlayRpc,
    transport::loopback_gateways, OverlayNodeConfig,
};
use serde_json::json;

#[test]
fn handles_create_vertex_via_jsonrpc() {
    let mut config = OverlayNodeConfig::default();
    config.networking.peers.clear();
    let (server, mut client) = loopback_gateways(&config.qstp).expect("gateways");
    let mut node = OverlayNode::new(config.clone(), server);
    let qeh = QehConfig::default();
    let icosuple = Icosuple::synthesize(&qeh, "vertex-alpha", 1024, 0.5);
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "privacynet_createVertex",
        "params": {
            "icosuple_json": serde_json::to_value(&icosuple).unwrap()
        }
    });
    client.seal_json(&request).expect("send");
    assert!(node.try_handle_qstp().expect("qstp").is_some());
    let response = client.try_recv_json().expect("recv").expect("json");
    assert_eq!(response["result"]["payload_bytes"], 1024);
}

#[test]
fn prove_and_verify_flow() {
    let prove_request = json!({
        "jsonrpc": "2.0",
        "id": 99,
        "method": "privacynet_proveAttribute",
        "params": {
            "did": "did:autheo:test",
            "attribute": "age > 21",
            "witness": "kyc-record-42"
        }
    });
    let mut config = OverlayNodeConfig::default();
    config.networking.peers.clear();
    config.privacynet.budget.session_epsilon = 1.0;
    config.privacynet.budget.session_delta = 1e-6;
    config.privacynet.budget.max_queries_per_session = 64;
    let (server, _) = loopback_gateways(&config.qstp).expect("gateways");
    let mut node = OverlayNode::new(config, server);

    let response = node
        .handle_jsonrpc(&prove_request.to_string())
        .expect("prove");
    let prove_json: serde_json::Value = serde_json::from_str(&response).unwrap();
    let proof_id = prove_json["result"]["proof_id"]
        .as_str()
        .expect("proof id")
        .to_string();

    let verify = json!({
        "jsonrpc": "2.0",
        "id": 100,
        "method": "privacynet_verifyProof",
        "params": {
            "proof_id": proof_id,
            "include_telemetry": true
        }
    });
    let verify_resp = node.handle_jsonrpc(&verify.to_string()).expect("verify");
    let verify_json: serde_json::Value = serde_json::from_str(&verify_resp).unwrap();
    assert!(verify_json["result"]["valid"].as_bool().unwrap());
    assert!(verify_json["result"]["telemetry"].is_object());
}

#[test]
fn grapplang_produces_rpc() {
    let rpc = parse_statement("revoke credential $cred before 2030-01-01").unwrap();
    match rpc {
        OverlayRpc::RevokeCredential(params) => {
            assert_eq!(params.credential_id, "cred");
            assert!(params.reason.unwrap().contains("2030"));
        }
        _ => panic!("expected revoke rpc"),
    }
}
