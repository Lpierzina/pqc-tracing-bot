# autheo-mldsa-falcon

`autheo-mldsa-falcon` packages Falcon-512/1024 signatures for PQCNet consumers. Like the Kyber and Dilithium crates, it now exposes a `liboqs`-backed implementation for native deployments and a deterministic fallback for `no_std` hosts.

## Code flow

```text
[liboqs::sig::Sig (Falcon512/1024)]
        |
        v
FalconLibOqs::{keypair,sign,verify}
        |
        v
autheo-pqc-core::adapters::MlDsa (Falcon mode)
        |
        v
Signature manager + PQC runtime (autheo-pqc-core / sentry / relayer)
```

The deterministic module keeps the legacy behavior that feeds `autheo-pqc-core`’s WASM surface, but its documentation now clearly treats it as a fallback; the production story is the liboqs adapter.

## Feature matrix

| Feature | Default? | Purpose |
| --- | --- | --- |
| `liboqs` | No (enable for native nodes) | Pulls in liboqs Falcon512/1024 bindings and exposes `FalconLibOqs`. |
| `deterministic` | Yes | Predictable Blake2s-based fallback used when compiling to `wasm32-unknown-unknown`. |
| `std` | Enabled by `liboqs` | Ensures `std` is available when linking to liboqs. |

## Usage

```rust
use autheo_mldsa_falcon::{FalconAlgorithm, FalconLibOqs};

let engine = FalconLibOqs::new(FalconAlgorithm::Falcon1024);
let pair = engine.keypair()?;
let sig = engine.sign(&pair.secret_key, b"epoch attest")?;
engine.verify(&pair.public_key, b"epoch attest", &sig)?;
```

## Fit within PQCNet

- `autheo-pqc-core` shares the same feature wiring as the Kyber and Dilithium crates: deterministic by default, liboqs when the host enables `--features liboqs`.
- `FalconLevel` now exposes `Falcon512` and `Falcon1024` so PQCNet surfaces can accurately tag security categories in receipts, telemetry, and `pqc_sign` ABIs.
- Chronosync/5DQEH modules that rely on Falcon signatures through `autheo-pqc-core` automatically inherit the upgraded documentation + liboqs binding without any code churn.

## Testing

- `cargo test -p autheo-mldsa-falcon` validates deterministic signing.
- `cargo test -p autheo-mldsa-falcon --features liboqs -- --ignored` runs the liboqs round trip.
