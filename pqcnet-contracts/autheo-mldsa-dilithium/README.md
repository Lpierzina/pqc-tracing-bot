# autheo-mldsa-dilithium

`autheo-mldsa-dilithium` delivers Autheo’s ML-DSA (Dilithium2/3/5) engine as a standalone crate. The default build keeps the deterministic fallback for WASM targets, while the `liboqs` feature exposes audited Dilithium bindings so native deployments share the exact implementation that powers `autheo-pqc-core::liboqs`.

## Code flow

```text
[liboqs::sig::Sig (Dilithium2/3/5)]
        |
        v
DilithiumLibOqs::{keypair,sign,verify}
        |
        v
autheo-pqc-core::adapters::MlDsa
        |
        v
Signature manager + PQC rotation APIs
```

The deterministic module (`DilithiumDeterministic`) mirrors this API using a fixed Blake2s transcript so `wasm32` builds (and browser demos) remain reproducible without embedding `liboqs`.

## Feature matrix

| Feature | Default? | Purpose |
| --- | --- | --- |
| `liboqs` | No (opt-in for native builds) | Links liboqs, exposes `DilithiumLibOqs` with ML-DSA-44/65/87. |
| `deterministic` | Yes | Keeps predictable, `no_std` Dilithium adapters for `autheo-pqc-wasm`. |
| `std` | Pulled in by `liboqs` | Enables `std` so the liboqs bindings can allocate and seed the runtime. |

## Usage

```rust
use autheo_mldsa_dilithium::{DilithiumAlgorithm, DilithiumLibOqs};

let engine = DilithiumLibOqs::new(DilithiumAlgorithm::MlDsa65);
let pair = engine.keypair()?;
let sig = engine.sign(&pair.secret_key, b"autheo proof")?;
engine.verify(&pair.public_key, b"autheo proof", &sig)?;
```

Deterministic fallback:

```rust
use autheo_mldsa_dilithium::DilithiumDeterministic;

let engine = DilithiumDeterministic::new();
let pair = engine.keypair()?;
let sig = engine.sign(&pair.secret_key, b"payload")?;
```

## Fit within PQCNet

- `autheo-pqc-core` consumes the deterministic feature set by default so WASM artifacts remain portable; enabling the `liboqs` feature on `autheo-pqc-core` cascades into this crate and `autheo-mldsa-falcon`.
- The shared `DilithiumLevel` enum feeds directly into `autheo-pqc-core::types::SecurityLevel`, so swapping ML-DSA-44/65/87 automatically propagates to the PQC handshake metadata and QS-DAG receipts.
- The `wasm-demo` assets inside `autheo-mldsa-dilithium/wasm-demo` keep building without `std`, ensuring browser showcases stay deterministic even as the top-level crate targets professional liboqs bindings.

## Testing

- `cargo test -p autheo-mldsa-dilithium` validates deterministic key/sig sizing plus tamper detection.
- `cargo test -p autheo-mldsa-dilithium --features liboqs -- --ignored` executes the liboqs-backed sign/verify round trip (native targets only).
