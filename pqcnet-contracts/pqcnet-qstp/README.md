## QSTP (Quantum-Secure Transport Protocol)

`pqcnet-qstp` packages the tunnel runtime used across PQCNet into a standalone crate designed for establishing secure, ephemeral channels that facilitate data transmission with zero-knowledge assurances. It binds Kyber + Dilithium handshakes to AES-256-GCM payloads, encrypts TupleChain descriptors before they ever leave the host, and leverages QACE to keep routes fresh without repeating the PQC ceremony.

### Highlights

- `establish_runtime_tunnel` turns PQC handshake envelopes into ready-to-use tunnels plus protobuf-friendly metadata blobs (`QstpPeerMetadata`) that remote peers can hydrate with a single call to `hydrate_remote_tunnel`.
- `QstpTunnel::seal/open` authenticate every frame against the active `MeshRoutePlan`, binding `TunnelId`, route hash, and application AAD into the AES-GCM transcript.
- `TupleChainStore` provides an integration point for storing encrypted metadata off-chain so operators can prove tunnel provenance without exposing plaintext—perfect for zero-knowledge attestations.
- `MeshTransport` + `InMemoryMesh` let you simulate Waku/THEO overlays and plug in real transports later.
- QACE interoperability via `register_alternate_routes` and `apply_qace` keeps channels adaptive: reroutes trigger nonce rotation, while rekeys keep tuple material in lock-step.

### How it works

```mermaid
sequenceDiagram
    participant Host as PQC Host
    participant Handshake as Kyber/Dilithium Handshake
    participant Tunnel as QSTP Tunnel
    participant Tuple as TupleChain Store
    participant Mesh as Mesh/Waku Overlay
    participant Peer as Remote Peer

    Host->>Handshake: build_handshake_artifacts(request)
    Handshake-->>Host: ciphertext + shared_secret + signature
    Host->>Tunnel: establish_runtime_tunnel(peer, route, tuple_store)
    Tunnel->>Tuple: encrypt metadata (zero-knowledge pointer)
    Tunnel->>Mesh: QstpFrame (AES-256-GCM, bound to route_hash)
    Mesh-->>Peer: sealed frame / peer metadata
    Peer->>Tunnel: hydrate_remote_tunnel(shared_secret, metadata, route)
    Tunnel->>Mesh: apply_qace(metrics) for reroute/rekey decisions
```

Every payload follows the same lifecycle: PQC handshake output → tunnel finalization → encrypted TupleChain metadata → sealed frames on the mesh. Observers never see tuple details in plaintext, yet auditors can later prove session lineage using the stored pointer and `QstpTunnel::fetch_tuple_metadata`.

### Usage

```rust
use pqcnet_qstp::{
    establish_runtime_tunnel, hydrate_remote_tunnel, InMemoryTupleChain, MeshPeerId,
    MeshQosClass, MeshRoutePlan, TunnelRole,
};

let mut tuple_store = InMemoryTupleChain::new();
let initiator_peer = MeshPeerId::derive("initiator");
let responder_peer = MeshPeerId::derive("responder");
let route = MeshRoutePlan {
    topic: "waku/alpha".into(),
    hops: vec![MeshPeerId::derive("hop-a")],
    qos: MeshQosClass::LowLatency,
    epoch: 1,
};

let mut initiator = establish_runtime_tunnel(
    b"client=demo&ts=1700",
    initiator_peer,
    route.clone(),
    &mut tuple_store,
)?;

let mut responder = hydrate_remote_tunnel(
    initiator.session_secret.clone(),
    responder_peer,
    route,
    initiator.peer_metadata.clone(),
    TunnelRole::Responder,
)?;

let frame = initiator.tunnel.seal(b"hello-qstp", b"app")?;
let clear = responder.open(&frame, b"app")?;
assert_eq!(clear, b"hello-qstp");
```

### Demos & Tests

| Command | Description |
| --- | --- |
| `cargo run -p pqcnet-qstp --example qstp_mesh_sim` | Spins up two peers over the in-memory mesh, exercises QACE reroutes, and shows tuple metadata retrieval. |
| `cargo run -p pqcnet-qstp --example qstp_performance` | Benchmarks QSTP handshake + payload latency versus a TLS baseline using `rustls`. |
| `cargo test -p pqcnet-qstp` | Runs the in-crate test suite, covering reroutes, tuple metadata proofs, and authenticity checks. |

Use these artifacts to prove the zero-knowledge and adaptive-routing guarantees end-to-end before wiring QSTP into external repos.
