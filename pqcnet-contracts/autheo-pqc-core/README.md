# autheo-pqc-core

Autheo's **PQC core enclave** wraps ML-KEM (Kyber) + ML-DSA (Dilithium/Falcon) engines, key rotation policies, transcript signing, and the WASM ABI that PQCNet runtimes embed. It sits between the entropy/QRNG layer and the TupleChain → Chronosync → 5D-QEH data plane, ensuring every tunnel, relay, and DAG anchor uses the exact same key material captured from production validators.

- Default builds enable the `real_data` feature, so the crate boots with the latest `data/qfkh_prod_trace.json` samples and replays the same Quantum-Forward Key Hopping (QFKH) telemetry that validators see.
- Deterministic demo engines remain available for reproducible tests; native deployments can enable the optional `liboqs` feature to bind audited Kyber/Dilithium implementations without touching contract logic.

## Responsibilities inside the PQCNet Suite

| Layer | Role inside PQCNet |
| --- | --- |
| Engines & entropy (`autheo-mlkem-kyber`, `autheo-mldsa-*`, `autheo-entropy-*`, `pqcnet-entropy`) | Provide deterministic/adaptable ML-KEM + ML-DSA adapters and host entropy feeds consumed by `autheo-pqc-core`.
| Core enclave (`autheo-pqc-core`, `autheo-pqc-wasm`) | Compose engines, threshold key management, ML-DSA signing, and expose the ABI (`pqc_alloc`, `pqc_free`, `pqc_handshake`).
| Controllers (`pqcnet-qfkh`, `pqcnet-qstp`, `pqcnet-qs-dag`, `pqcnet-crypto`) | Consume the enclave’s key states, ciphertexts, and transcripts to drive tunnels, DAG anchors, and sign-and-exchange flows.
| Data plane (`autheo-pqcnet-{tuplechain,icosuple,chronosync,5dqeh}`) | Reuse the PQC artifacts to stamp TupleChain receipts, Chronosync elections, and 5D-QEH anchors before relaying outward.

## Real Data Code Flow

The diagram below shows how real telemetry from validators feeds the enclave and propagates through PQCNet runtime crates.

flowchart LR
    Trace["data/qfkh_prod_trace.json<br/>(real validator telemetry)"]
    Build["build.rs<br/>serde + hex embed"]
    Recorded["runtime::recorded<br/>RECORDED_SAMPLES"]
    KeyMgr["key_manager::KeyManager<br/>(t-of-n rotation)"]
    SigMgr["signatures::SignatureManager<br/>ML-DSA transcripts"]
    Handshake["handshake::execute_handshake<br/>&amp;pqc_handshake ABI"]
    QFkh["pqcnet-qfkh<br/>(epoch hop controller)"]
    Qstp["pqcnet-qstp<br/>QSTP tunnels"]
    QsDag["pqcnet-qs-dag<br/>anchor helpers"]
    Tuple["autheo-pqcnet-tuplechain -> Chronosync -> 5D-QEH"]
    Relayer["pqcnet-relayer + pqcnet-telemetry"]

    Trace --> Build --> Recorded --> KeyMgr -->|KeyId + ciphertext| Handshake
    Recorded --> SigMgr -->|transcript sigs| Handshake
    Handshake -->|Key frames| Qstp --> Relayer
    KeyMgr --> QFkh --> Tuple --> QsDag --> Relayer
    SigMgr --> QsDag


**What this guarantees**

1. `build.rs` parses the prod trace, emits `recorded_trace.rs`, and locks in `TRACE_ROTATION_INTERVAL_MS`/`RECORDED_SAMPLES` for every build.
2. `runtime::recorded::build_contract_state` wires those constants into `KeyManager` + `SignatureManager`, so the enclave boots with real ML-KEM keypairs, ciphertexts, and transcript signatures.
3. `pqc_handshake` returns envelopes that `pqcnet-qstp`, `pqcnet-qfkh`, and `pqcnet-qs-dag` consume without any “simulator” shortcuts—every test hits the same path as production nodes.

Disable the telemetry replay only when you need deterministic demos:

```bash
cargo test -p autheo-pqc-core --no-default-features
```

Enable audited engines via liboqs (native targets only):

```bash
cargo run -p autheo-pqc-core --bin liboqs_cli --features liboqs -- --message "veil handshake"
```

## Module Map

| File | Purpose |
| --- | --- |
| `src/adapters.rs` | Deterministic ML-KEM/ML-DSA stand-ins used for WASM builds and CI.
| `src/kem.rs`, `src/dsa.rs` | Engine traits + wrappers that host implementations plug into.
| `src/key_manager.rs` | Threshold rotation policy (`ThresholdPolicy { t, n }`), rotation timers, and encapsulation helpers.
| `src/signatures.rs` | Signature lifecycle + batch verification and transcript signing for QSTP/QS-DAG.
| `src/handshake.rs` | Host-facing handshake orchestration plus transcript encoding used by the WASM ABI.
| `src/runtime.rs` & `src/runtime/recorded.rs` | Contract state builder, recorded trace bootstrap, and `pqc_handshake` entrypoints.
| `src/secret_sharing.rs` | Shamir helpers (native only) for splitting ML-KEM private keys inside validator tooling.
| `tests/examples.rs` & `examples/*.rs` | Reproducible demos (handshake, secret sharing) that mirror validator/relayer flows.

## Embedding into PQCNet runtimes

1. **Entropy** – Host runtimes must supply `pqcnet-entropy::HostEntropySource`, typically backed by `autheo-entropy-core` + `autheo-entropy-wasm`, so `KeyManager` and transcript signing stay on production entropy budgets.
2. **QFKH** – `pqcnet-qfkh` verifies `data/qfkh_prod_trace.json` at build time; keep the copy in sync to ensure both crates replay identical epochs.
3. **QSTP** – `pqcnet-qstp` parses the handshake envelope, hydrates `QstpTunnel`s, and binds QACE-controlled route metadata without re-running ML-KEM.
4. **QS-DAG** – `pqcnet-qs-dag::QsDagPqc::verify_and_anchor` uses `SignatureManager::verify` callbacks from this crate to attach ML-DSA proofs onto Chronosync/5D-QEH edges.
5. **Relayers & telemetry** – The outputs feed `pqcnet-relayer` and `pqcnet-telemetry`, ensuring off-box analytics see the same KeyIds, epochs, and rotation timestamps that the enclave emitted.

## Build, Test, and Diagnose

```bash
# Standard build (real telemetry replay)
cargo build -p autheo-pqc-core

# Deterministic adapters for reproducible CI runs
cargo test -p autheo-pqc-core --no-default-features

# Run the handshake + QSTP demo end-to-end
cargo run -p autheo-pqc-core --example handshake_demo

# Exercise Shamir helpers (native targets only)
cargo test -p autheo-pqc-core secret_sharing
cargo run  -p autheo-pqc-core --example secret_sharing_demo
```

- `wazero-harness/` consumes the compiled WASM (`autheo_pqc_wasm.wasm`) and compares telemetry counters to production validators.
- `docs/qstp.md` and `docs/pqcnet-architecture-integration.md` contain extended diagrams that reference this crate; keep them aligned whenever you change the handshake layout or contract surface.

## When to touch this crate

- Updating ML-KEM/ML-DSA adapters or swapping in audited engines.
- Changing key rotation intervals, threshold policies, or transcript layouts.
- Refreshing recorded telemetry (`data/qfkh_prod_trace.json`) so the default build reflects the latest validators.
- Extending the WASM ABI surface (e.g., adding new host calls) before plumbing those changes through `autheo-pqc-wasm`, `pqcnet-qstp`, and the wazero harness.
