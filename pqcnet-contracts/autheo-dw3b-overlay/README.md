# Autheo DW3B Overlay

`autheo-dw3b-overlay` exposes the Dark Web Privacy Network (DW3B) facade that
wraps the `autheo-dw3b-mesh` engine with transports, JSON-RPC 2.0 endpoints, and
Zer0veil/Grapplang bindings. The overlay mirrors the methods described in the
PrivacyNet + DW3B Mesh primer (`dw3b_anonymizeQuery`, `dw3b_obfuscateRoute`,
`dw3b_policyConfigure`, `dw3b_entropyRequest`, `dw3b_qtaidProve`, `dw3b_syncState`)
so control planes can interact with the privacy stack using the exact API
surface documented in the spec.

## Features

- **Mesh orchestration** – embeds `Dw3bMeshEngine`, converts RPC payloads into
  anonymization requests, and publishes overlay frames (vertex created, proof
  generated, entropy beacon, policy changes) through `pqcnet-networking`.
- **QSTP sealing** – `transport::QstpGateway` encapsulates JSON-RPC frames inside
  AES-GCM envelopes derived from QSTP tunnels, mirroring the production
  loopback/harness strategy.
- **DW3B RPC schema** – `rpc` module implements the JSON-RPC 2.0 contracts for
  anonymize, obfuscate, policy configure, entropy request, state sync, and QTAID
  proofs. Responses include AnonymityProof objects, Bloom summaries, and mesh
  telemetry to keep sentries/relayers honest.
- **Grapplang parsing** – `grapplang` translates Zer0veil shell commands into the
  corresponding RPC requests (e.g., `dw3b-anonymize`, `qtaid-prove`,
  `dw3b-policy`).
- **Telemetry + networking** – integrates with `pqcnet-telemetry` for latency/
  counter recording and `pqcnet-networking` for gossiping overlay frames across
  DW3B observers.

## Code flow

```mermaid
---
config:
  theme: forest
---
flowchart LR
    Grapplang["Grapplang CLI / Zer0veil shell"]
    JsonRpc["JSON-RPC 2.0 request"]
    Gateway["Dw3bGateway\n(QSTP tunnel)"]
    Overlay["Dw3bOverlayNode"]
    Mesh["Dw3bMeshEngine"]
    PrivacyNet["autheo-privacynet\npipeline"]
    Networking["pqcnet-networking"]
    Telemetry["pqcnet-telemetry"]
    Frames["OverlayFrame broadcast"]

    Grapplang -->|parse_statement| JsonRpc
    JsonRpc -->|dw3b_* methods| Overlay
    Gateway -->|sealed frame| Overlay
    Overlay -->|MeshAnonymizeRequest| Mesh
    Mesh -->|DW3B proof + entropy| Overlay
    Mesh --> PrivacyNet
    Overlay -->|OverlayFrame events| Networking --> Frames
    Overlay -->|metrics| Telemetry
    Overlay -->|JSON responses| Gateway
```

The overlay accepts either Grapplang statements or JSON-RPC envelopes, clamps
privacy budgets, runs the DW3B mesh engine, and then uses QSTP sealed frames to
return proofs plus telemetry-backed overlay events. Everything is deterministic
under `Dw3bOverlayConfig::demo()`, which keeps docs, examples, and tests aligned.

## Quick start

```rust
use autheo_dw3b_overlay::{
    config::Dw3bOverlayConfig,
    overlay::Dw3bOverlayNode,
    transport::loopback_gateways,
};

let config = Dw3bOverlayConfig::demo();
let (gateway, _) = loopback_gateways(&config.qstp).unwrap();
let mut node = Dw3bOverlayNode::new(config, gateway);
let response = node.handle_jsonrpc(r#"{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "dw3b_anonymizeQuery",
    "params": {
        "did": "did:autheo:demo",
        "attribute": "age > 18",
        "payload": "kyc-record-42",
        "epsilon": 1e-6,
        "delta": 8.7e-13,
        "route_layers": 5
    }
}"#).unwrap();
println!("{response}");
```

## Examples

- **Loopback JSON-RPC** – run `cargo run -p autheo-dw3b-overlay --example loopback_overlay`
  to see the overlay handle a JSON-RPC anonymize request, answer an entropy beacon
  that arrived over the loopback QSTP tunnel, and parse Grapplang commands. The
  example prints the proof id, entropy sample count, and the parsed Grapplang
  options so shell tooling can mirror the same flow.

## Testing

```
cargo test -p autheo-dw3b-overlay
```

`tests/overlay.rs` now drives anonymize + QTAID flows, verifies Grapplang parsing,
and exercises the QSTP loopback entropy path. The added coverage ensures the
overlay clamps privacy budgets, parses CLI statements, and returns VRB beacons
exactly the way the README diagrams describe.
