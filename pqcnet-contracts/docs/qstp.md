# QSTP Tunnels

The `autheo-pqc-core::qstp` module turns the Kyber + Dilithium handshake
artifacts into long-lived quantum-secure data channels with Waku-compatible
transport semantics, TupleChain storage hooks, and a programmable QACE
(Quantum Adaptive Channel Engineering) controller.

## Establishing A Tunnel

- `establish_runtime_tunnel(request, peer, route, tuple_store)` drives the
  existing ML-KEM + ML-DSA handshake inside the WASM runtime, signs the
  transcript, and derives:
  - `TunnelId` (`blake2(ciphertext || signature || route_hash)`)
  - Directional AES-256-GCM keys and nonces for `QstpTunnel::seal/open`
  - Encrypted TupleChain metadata (threshold policy, key identifiers, route hash)
- The response (`QstpEstablishedTunnel`) includes:
  - `tunnel`: ready-to-use `QstpTunnel`
  - `peer_metadata`: fields mapped 1:1 into `protos/qstp.proto`
  - `session_secret`: handed to higher layers (e.g., TupleChain clients) so they
    can hydrate their own responders via `hydrate_remote_tunnel`

Hydrating a responder requires the shared secret (decapsulated by the peer),
the advertised route, and the `QstpPeerMetadata` blob.  Both sides derive
directional AES keys from the same salt (`TunnelId || route_hash || epoch`) and
retain their role (`Initiator` vs `Responder`) to keep send/receive nonces in
lock-step.

## Data Plane & Mesh Integration

- `QstpTunnel::seal` / `open` enforce the PQC transcript, binds every payload to
  the active route hash, and emits a `QstpFrame` (topic + seq + nonce + ciphertext)
- `MeshTransport` defines the light-weight contract expected from Waku-style
  overlays (`publish`/`try_recv`).  `InMemoryMesh` powers the simulator.
- `qstp.proto` mirrors these types so other languages can gossip handshakes,
  frames, and QACE notifications without re-implementing the binary envelope.

## QACE (Adaptive Routing)

- `GeneticQace` is a pluggable controller that evaluates latency/loss/threat
  metrics and emits `QaceDecision`s.
- `QstpTunnel::apply_qace` consumes the decision, rotates directional nonces,
  updates the current `MeshRoutePlan`, and records the last action.
- When the threat score crosses the high watermark, the tunnel immediately
  switches to the next registered alternate route (and re-derives AES bases
  without rekeying ML-KEM / ML-DSA).

## TupleChain Storage

- `TupleChainStore` is the trait hosts implement.  `InMemoryTupleChain` gives a
  dev-friendly reference.
- Metadata is AES-GCM protected with a key derived from the session secret and
  the tunnel id.  `QstpTunnel::fetch_tuple_metadata` proves the TupleChain entry
  can be recovered by either endpoint without exposing plaintext to intermediaries.

## Examples & Tests

```
cargo run -p autheo-pqc-core --example qstp_mesh_sim
cargo run -p autheo-pqc-core --example qstp_performance
cargo test -p autheo-pqc-core qstp::tests::qstp_rerouted_payload_decrypts
```

The mesh simulator wires two endpoints through the in-memory Waku harness,
triggers a QACE reroute, proves that only the legitimate responder can decrypt
the rerouted frame, and verifies the TupleChain pointer that was persisted during
the handshake.

The performance harness compares the tunnel runtime with a TLS baseline (see
`docs/qstp-performance.md` for the captured numbers).

## Proto Contract

The protobuf contract lives in `protos/qstp.proto` and mirrors the Rust types:
`HandshakeEnvelope`, `QstpFrame`, `TupleMetadata`, and `QaceReport/QaceDecision`.
Example clients can feed the `peer_metadata`+`session_secret` pairs emitted by
`establish_runtime_tunnel` into their own decapsulation + hydration flows
without needing to deserialize the PQC1 binary layout by hand.
