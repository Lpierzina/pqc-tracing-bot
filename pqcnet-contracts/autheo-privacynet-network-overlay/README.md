# Autheo PrivacyNet – Network Overlay

Production-grade shell that wraps the `autheo-privacynet` engine with the transports, RPC surface, and shell grammar described in the Autheo PrivacyNet primer. The crate exposes a thin distributed overlay so control planes can issue JSON-RPC 2.0 or Grapplang statements and have proofs executed over PQCNet/QSTP tunnels.

## How It Works

- **OverlayNode** – main orchestrator. It accepts `OverlayRpc` requests, validates payload limits, forwards the work to `PrivacyNetEngine`, manages privacy budgets, and pushes telemetry/overlay events over `pqcnet-networking`.
- **QSTP Gateways** – `transport::QstpGateway` seals JSON-RPC envelopes into AES-GCM frames derived from QSTP tunnel state. The `loopback_gateways` helper lets tests run without a live mesh.
- **RPC & Grapplang** – `rpc` defines the JSON-RPC methods (`privacynet_createVertex`, `privacynet_proveAttribute`, etc.), while `grapplang::parse_statement` converts Zer0veil shell statements (prove/verify/revoke/qtaid) into those RPCs.
- **Overlay Frames** – whenever vertices, proofs, revocations, or QTAID tokenizations occur, the overlay emits `OverlayFrame` events across the networking facade so sentries/relayers can fan them out.

### Code Flow Diagram

```mermaid
flowchart LR
    Client[[JSON-RPC or Grapplang]] -->|parse_statement / decode_request| OverlayNode
    OverlayNode -->|compose_request| PrivacyNetEngine
    PrivacyNetEngine -->|handle_request| {DP Engine / FHE / EZPH Pipeline}
    {DP Engine / FHE / EZPH Pipeline} -->|PrivacyNetResponse| OverlayNode
    OverlayNode -->|OverlayFrame + telemetry| PQCNetNetworking
    OverlayNode -->|seal_json| QSTPGateway
    QSTPGateway -->|QSTP Frames| MeshTransport
```

## Quick Start

```rust
use autheo_privacynet_network_overlay::{
    overlay::OverlayNode,
    transport::loopback_gateways,
    OverlayNodeConfig,
};

let config = OverlayNodeConfig::default();
let (gateway, _) = loopback_gateways(&config.qstp).expect("qstp loopback");
let mut node = OverlayNode::new(config, gateway);
let response = node.handle_jsonrpc(r#"{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "privacynet_proveAttribute",
    "params": {
        "did": "did:autheo:demo",
        "attribute": "age > 21",
        "witness": "kyc-record-42"
    }
}"#).unwrap();
println!("{response}");
```

### Grapplang Shell Examples

```
prove age > 18 from did:autheo:abc123 using manifold=5d-chaos
verify proof $PROOF on vertex $VERTEX
revoke credential $CRED before 2030-01-01
qtaid prove "BRCA1=negative" from genome $GENOME
```

## Testing

The crate includes unit tests for the Grapplang parser and integration tests that run the full overlay against loopback QSTP transports.

```
cd pqcnet-contracts
cargo test -p autheo-privacynet-network-overlay
```

The integration suite (`tests/overlay.rs`) covers:
- `privacynet_createVertex` round-tripping over QSTP loopback
- `privacynet_proveAttribute` + `privacynet_verifyProof` using the real PrivacyNet engine pipeline
- Grapplang statement parsing for prove/verify/revoke/qtaid flows
