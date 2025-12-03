use autheo_dw3b_overlay::{
    config::Dw3bOverlayConfig, parse_statement, Dw3bOverlayNode, Dw3bOverlayRpc, OverlayResult,
    loopback_gateways,
};
use serde_json::json;

fn main() -> OverlayResult<()> {
    let config = Dw3bOverlayConfig::demo();
    let (gateway, mut remote) = loopback_gateways(&config.qstp)?;
    let mut node = Dw3bOverlayNode::new(config, gateway);

    // Direct JSON-RPC anonymize request
    let anonymize = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "dw3b_anonymizeQuery",
        "params": {
            "did": "did:autheo:demo",
            "attribute": "age > 21",
            "payload": "kyc-record-99",
            "epsilon": 1e-6,
            "delta": 2e-12,
            "route_layers": 4
        }
    });
    let response = node.handle_jsonrpc(&anonymize.to_string())?;
    println!("dw3b_anonymizeQuery → {response}");

    // QSTP loopback request (remote peer sends JSON, overlay responds after sealing)
    let entropy = json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "dw3b_entropyRequest",
        "params": { "samples": 2, "dimension5": true }
    });
    remote.seal_json(&entropy)?;
    if let Some(response) = node.try_handle_qstp()? {
        let count = response["result"]["vrbs"]
            .as_array()
            .map(|arr| arr.len())
            .unwrap_or_default();
        println!("loopback entropy samples={count}");
    }

    // Grapplang parsing mirrors Zer0veil shells
    match parse_statement("dw3b-anonymize proof --route-layers 6 --did did:autheo:test")? {
        Dw3bOverlayRpc::AnonymizeQuery(params) => {
            println!(
                "grapplang → route_layers={} did={}",
                params.route_layers, params.did
            );
        }
        other => {
            println!("parsed different rpc variant: {:?}", other);
        }
    }

    Ok(())
}
