# QSTP Tunnels

The `pqcnet_qstp` crate turns the Kyber + Dilithium handshake
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

All of the above executes under the Autheo WASM Runtime Engine (AWRE) with WAVEN’s dual page-table MMU. The wasm-micro-runtime build ships with interpreter/AOT/JIT tiers tuned for sub-MB enclaves, and the WAVEN layer provides exception pages plus page-level sharing so PQCNet overlays can co-host multiple tenants without reworking bounds-checking. The runtime is seeded through `qrng_feed` before `establish_runtime_tunnel` runs so every session inherits an attested entropy tuple (ABW34) the DAO can audit.
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
  overlays (`publish`/`try_recv`). Bring your own transport implementation—this
  crate no longer ships any simulators.
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
cargo test -p pqcnet-qstp
cargo test -p pqcnet-qstp qace_rekey_rotates_nonce_material
cd wazero-harness && go run . -wasm ../pqcnet-contracts/target/wasm32-unknown-unknown/release/autheo_pqc_wasm.wasm
```

- The default `cargo test` run covers tuple proofs, QACE reroutes, eavesdropper
  detection, and TupleChain fetches without touching any simulators.
- The focused `qace_rekey_rotates_nonce_material` test asserts that QACE rekey
  actions rotate nonce bases without diverging tunnel state.
- The Go-based `wazero-harness` invocation is the canonical host-level smoke
  test: it loads the WASM contract, invokes the same handshake ABI, and validates
  persisted TupleChain pointers against QS-DAG semantics—all without relying on
  synthetic overlays. The harness also stamps the AWRE profile hash, WAVEN dual page-table status, and `qrng_feed` tuple ID into ABW34 telemetry so you can prove the test used the same runtime as production deployments.

## Runtime Parity (AWRE + WAVEN)

- **Profile + verification** – Ship `AWRE_PROFILE=awre-waven` (or `--awre-profile awre-waven`) with every QSTP node. Run `scripts/awre_waven_verify.sh` in CI and during rollouts; it enforces the wasm-micro-runtime commit, WAVEN MMU toggles, and qrng_feed wiring before tunnels spin up.
- **Telemetry coupling** – `pqcnet-telemetry` exports the AWRE/WAVEN measurement hash and ABW34 tuple id next to tunnel latency metrics. DAO observers can correlate those fields with governance proposals to confirm that any tunnel diff kept the sanctioned runtime story.
- **Harness reuse** – The same AWRE + WAVEN pairing powers the wazero harness, the QuTiP CHSH sandbox, and the validator workloads referenced in `README.md`. That keeps regression data, documentation, and validator evidence aligned when auditors ask how QSTP interacts with the enclave runtime.

## Proto Contract

The protobuf contract lives in `protos/qstp.proto` and mirrors the Rust types:
`HandshakeEnvelope`, `QstpFrame`, `TupleMetadata`, and `QaceReport/QaceDecision`.
Example clients can feed the `peer_metadata`+`session_secret` pairs emitted by
`establish_runtime_tunnel` into their own decapsulation + hydration flows
without needing to deserialize the PQC1 binary layout by hand.
