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

## Handshake Flow (Kyber + Dilithium)

Outside the WASM ABI you can now run the exact same ML-KEM/ML-DSA flow with the
new trio of helpers:

- `handshake::init_handshake` encapsulates to the responder’s ML-KEM public key,
  derives a deterministic initiator nonce from the route hash + ciphertext +
  application payload, and signs that transcript with the initiator’s ML-DSA key.
- `handshake::respond_handshake` verifies the initiator signature, decapsulates
  the shared secret, emits the responder nonce + session id, and signs its own
  transcript with the responder’s ML-DSA key.
- `handshake::derive_session_keys` finishes the flow for both roles.  The responder
  calls it immediately after `respond_handshake`; the initiator calls it after
  verifying the responder signature/nonce embedded in `HandshakeResponse`.

The example at `cargo run -p autheo-pqc-core --example handshake_demo` (or with
`--features liboqs` to swap Demo adapters for liboqs Kyber/Dilithium) prints the
shared session id, decrypts a sample payload, and dumps the tuple key so you can
prove both endpoints derived identical AES-256-GCM bases without ever sharing the
session secret on the wire.

`protos/qstp.proto` mirrors these envelopes through `HandshakeInit`,
`HandshakeResponse`, and `SessionKeyMaterial`, allowing non-Rust languages to
exchange the same signed artifacts before calling into their own AEAD layers.

## Data Plane & Mesh Integration

- `QstpTunnel::seal` / `open` enforce the PQC transcript, binds every payload to
  the active route hash, and emits a `QstpFrame` (topic + seq + nonce + ciphertext)
- `MeshTransport` defines the light-weight contract expected from Waku-style
  overlays (`publish`/`try_recv`).  `InMemoryMesh` powers the simulator.
- `qstp.proto` mirrors these types so other languages can gossip handshakes,
  frames, and QACE notifications without re-implementing the binary envelope.

## QACE (Adaptive Routing)

- `pqcnet_qace::GaQace` (genetic algorithm) and `pqcnet_qace::SimpleQace`
  (deterministic fallback) are the pluggable controllers that evaluate telemetry
  and emit new `PathSet`s.
- `QstpTunnel::apply_qace(qace.evaluate(paths, metrics))` consumes the decision,
  rotates directional nonces, updates the active `MeshRoutePlan`, and records
  the last action for observability.
- `QaceRoute` is the trait implemented by `MeshRoutePlan`, so any transport that
  exposes hop count, QoS bias, freshness, and viability can plug into QACE.
- The GA controller ingests multi-metric state (latency, loss, jitter,
  bandwidth, threat, chaos-level) and evolves a high-fitness ordering of routes.
  The recommended configuration mirrors the defaults in `QaceGaConfig`:
  - `population_size = 48` chromosomes, `max_generations = 64`,
    `max_stale_generations = 16`
  - `mutation_probability = 0.18`, `selection_rate = 0.6`, `crossover_rate = 0.75`
  - `replacement_rate = 0.65`, `elitism_rate = 0.04`, `tournament_size = 7`
  - `QaceWeights` bias low-latency and control QoS paths while penalising hop
    count and congestion. Adjust `hop_penalty` and `qos_gain` to match your mesh.
- `PathSet` captures the primary + alternates returned by the controller, making
  it easy to replicate the decision on the responder or log it in control-plane
  telemetry.
- Run the standalone examples to benchmark QACE behaviour:

  ```
  cargo run -p pqcnet-qace --example ga_failover
  cargo run -p pqcnet-qace --example deterministic_guard
  ```

  `ga_failover` prints the chosen primary path, GA fitness score, and convergence
  confidence for steady, congested, and threat-injection scenarios so you can
  validate the <50 ms / <10% overhead target from User Story 2 across chaos
  inputs, while `deterministic_guard` demonstrates the WASM-friendly fallback.

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
