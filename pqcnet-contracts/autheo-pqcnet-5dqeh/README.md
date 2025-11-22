# autheo-pqcnet-5dqeh

`autheo-pqcnet-5dqeh` is now treated as a chain module, not a simulation toy. The crate exposes the Five-Dimensional Qubit-Enhanced Hypergraph (5D-QEH) state machine, storage layout helpers, and RPC-friendly message types so Chronosync, TupleChain, and PQCNet runtimes can embed it directly (native or `wasm32-unknown-unknown`). The developer simulator remains in `examples/` solely as a diagnostic harness.

## Module scope

- **Consensus / QS-DAG hook** – `HypergraphModule::apply_anchor_edge` now powers the Chronosync keeper: it verifies 4096-byte icosuples, enforces ≤100 parents, recomputes temporal weight, and returns receipts that QS-DAG snapshots can persist.
- **Ledger affinity** – hot vs crystalline placement is tracked via `ModuleStorageLayout`, keeping TupleChain/Icosuple tiers aligned with Autheo’s storage policies.
- **PQC plumbing** – `PqcBinding` plus the new `PqcRuntime` trait call straight into `autheo-pqc-core`’s `pqc_handshake/pqc_sign/pqc_rotate` ABIs (native or WASM). `CorePqcRuntime` ships in-crate so chain modules can request signatures or rotations before admitting an anchor edge.
- **RPC / ABCI shape** – `MsgAnchorEdge`, `MsgAnchorEdgeResponse`, and the RPCNet router mirror the protobuf definitions in `protos/pqcnet_5dqeh.proto`/`qstp.proto`, so relayers, CLI clients, and ABCI handlers speak the same language.

## State machine + storage layout

```rust
use autheo_pqcnet_5dqeh::{
    HypergraphModule, MsgAnchorEdge, PqcBinding, PqcScheme, QehConfig, TemporalWeightModel,
};

let config = QehConfig::default();
let weight_model = TemporalWeightModel::default();
let mut module = HypergraphModule::new(config.clone(), weight_model);

let msg = MsgAnchorEdge {
    request_id: 42,
    chain_epoch: 7,
    parents: vec![],
    parent_coherence: 0.1,
    lamport: 1,
    contribution_score: 0.5,
    ann_similarity: 0.92,
    qrng_entropy_bits: 384,
    pqc_binding: PqcBinding::new("did:autheo:node/validator-01", PqcScheme::Dilithium),
    icosuple: build_icosuple_somewhere(),
};

let receipt = module.apply_anchor_edge(msg)?;
println!("vertex={} storage={:?}", receipt.vertex_id, receipt.storage);
println!(
    "hot={} crystalline={}",
    module.storage_layout().hot_vertices,
    module.storage_layout().crystalline_vertices
);
```

- `HypergraphModule` wraps the deterministic `HypergraphState` and enforces temporal-weight scoring for every edge.
- `ModuleStorageLayout` tracks hot/crystalline counts so host runtimes can write to their preferred backends.
- `VertexReceipt`/`HyperVertex` derive `serde` so receipts can be routed over RPC or persisted in telemetry logs.

## Chronosync keeper + RPCNet

- `ChronosyncKeeper` (in `autheo-pqcnet-chronosync`) feeds QS-DAG elections straight into this crate. Each `DagNode` becomes a `MsgAnchorEdge`, and the keeper records canonical vertices + storage counters while maintaining a DAG index for relayers.
- `RpcNetRouter` (from `pqcnet-networking`) is now aware of both `MsgAnchorEdge` and `MsgOpenTunnel`. Attach a keeper plus a tuple-store implementation and you instantly get JSON/REST/gRPC-ready endpoints for anchoring edges or opening QSTP tunnels.
- Every `VertexReceipt` optionally carries the PQC signature produced during anchoring, so relayers, sentries, or Chronosync watchers can forward the exact bytes that were signed via `pqc_sign`.

## RPC + schema

- The protobuf contract for node/ABCI integrations lives in `protos/pqcnet_5dqeh.proto` (`MsgAnchorEdge`, `MsgAnchorEdgeResponse`, `QehVertexReceipt`, `QehStorageLayout`, etc.).
- Each icosuple carries PQC metadata (`PqcLayer`, `PqcBinding`) so Autheo nodes can assert that Kyber/Dilithium/Falcon slots match the PQC engine active inside `autheo-pqc-core`/`autheo-pqc-wasm`.
- RPC handlers wrap the Rust structs one-to-one, making it trivial to expose REST/gRPC endpoints such as `POST /pqcnet/5dqeh/v1/anchor_edge`.

## Build targets

| Command | Description |
| --- | --- |
| `cargo build -p autheo-pqcnet-5dqeh` | Native build used by Autheo nodes and integration tests. |
| `cargo build -p autheo-pqcnet-5dqeh --target wasm32-unknown-unknown --no-default-features` | Produces the WASM artifact that relies on the host-imported entropy feed. Add `--features sim` only when you need the simulator. |

The crate stays `no_std` friendly whenever `std` is disabled so the same source can be embedded inside custom host environments.

## Host entropy + QRNG integration

- A dedicated `QrngEntropyRng` now powers the simulator. Enable the `sim` feature to source deterministic entropy from `pqcnet-entropy`'s `SimEntropySource` so you can replay fixed vectors locally.
- Production builds (default features) ship zero simulators. When compiling for `wasm32-unknown-unknown`, the crate unconditionally calls the host import `autheo_host_entropy(ptr, len)` which is satisfied by the standalone `autheo-entropy-wasm` node.
- Demo: `cargo run -p autheo-pqcnet-5dqeh --features sim --example host_entropy_demo` prints reproducible vertex IDs and telemetry. The classic simulator walkthrough still works via `cargo run -p autheo-pqcnet-5dqeh --features sim --example coherence_walkthrough`.
- Test guard: `cargo test -p autheo-pqcnet-5dqeh --features sim qrng_entropy_rng_is_deterministic_under_seed` asserts that the simulator RNG stays deterministic under a fixed seed.

In production the DePIN entropy node (`autheo-entropy-wasm`) is instantiated alongside each PQC module. Hosts seed it with hardware randomness (RPi, validator HSMs, etc.) and bridge every `autheo_host_entropy` call by copying bytes from the entropy module into the PQC module's linear memory.

## Dev harness (examples/)

Simulations are relegated to developer tooling. The `FiveDqehSim` helper drives `HypergraphModule` for telemetry but is not part of the production surface.

- `cargo run -p autheo-pqcnet-5dqeh --example coherence_walkthrough`
  - Prints per-epoch accept/archive counts, coherence, laser telemetry, and storage layout stats so you can validate parameter tweaks.

## Tests

- `cargo test -p autheo-pqcnet-5dqeh`
  - Verifies temporal-weight math, parent-limit enforcement, simulator telemetry, PQC runtime integration (`module_attaches_pqc_signature_when_runtime_available`), and that storage-layout accounting matches accepted vertices.

## Next steps

- Surface `ChronosyncKeeperReport` telemetry (storage deltas, PQC signatures, missing parents) over RPCNet so sentries can subscribe without scraping logs.
- Add slashing / alert hooks that fire whenever `missing_parents` is non-empty or PQC rotations fail, wiring them into relayer & telemetry crates.
- Bundle the `RpcNetRouter` into the relayer CLI so `MsgAnchorEdge`/`MsgOpenTunnel` can be exercised over HTTP/gRPC instead of direct library calls.
