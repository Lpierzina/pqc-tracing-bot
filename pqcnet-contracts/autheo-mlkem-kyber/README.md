# autheo-mlkem-kyber

`autheo-mlkem-kyber` is the Kyber (ML-KEM) engine that Autheo publishes as a standalone crate. It exposes hardened `liboqs` bindings for ML-KEM-512/768/1024 while retaining a deterministic, `no_std`-friendly fallback for `wasm32` deployments that cannot link against `liboqs`.

## Code flow

```text
[liboqs::kem::Kem (Kyber512/768/1024)]
        |
        v
KyberLibOqs::{keypair,encapsulate,decapsulate}
        |
        v
autheo-pqc-core::adapters::MlKem (SecurityLevel tagging)
        |
        v
PQC handshake + rotation ABIs (autheo-pqc-core / autheo-pqc-wasm)
```

When the `liboqs` feature is enabled (default for native builds), the crate instantiates `liboqs::kem::Kem`, normalises the byte representation, and feeds it directly into the PQCNet adapters. The deterministic module is compiled only when the consumer requests the `deterministic` feature (e.g., Autheo’s WASM builds) and mirrors the exact API, so higher layers flip features without code changes.

## Feature matrix

| Feature | Default? | Purpose |
| --- | --- | --- |
| `liboqs` | No (enable explicitly for native binaries) | Pulls in `oqs` + `std`, exposes `KyberLibOqs` with ML-KEM-512/768/1024 support. |
| `deterministic` | Yes | Keeps the deterministic, `no_std` fallback (`KyberDeterministic`) used by `autheo-pqc-core` when targeting `wasm32`. |
| `std` | Enabled automatically by `liboqs` | Opts the crate into `std` so `oqs` can allocate and initialise the C bindings. |

Native nodes typically compile with:

```bash
cargo build -p autheo-mlkem-kyber --features liboqs --release
```

The WASM demo artifacts use:

```bash
cargo build -p autheo-mlkem-kyber --target wasm32-unknown-unknown --no-default-features --features deterministic
```

## Usage

```rust
use autheo_mlkem_kyber::{KyberAlgorithm, KyberLibOqs};

let engine = KyberLibOqs::new(KyberAlgorithm::MlKem768);
let keypair = engine.keypair()?;
let enc = engine.encapsulate(&keypair.public_key)?;
let shared = engine.decapsulate(&keypair.secret_key, &enc.ciphertext)?;
assert_eq!(shared, enc.shared_secret);
```

For deterministic builds:

```rust
use autheo_mlkem_kyber::KyberDeterministic;

let engine = KyberDeterministic::new();
let keypair = engine.keypair()?;
```

## Fit within PQCNet

- `autheo-pqc-core` depends on this crate with `default-features = false, features = ["deterministic"]` so the WASM surface stays `no_std`.
- The core crate’s `liboqs` feature cascades into `autheo-mlkem-kyber/liboqs`, allowing hosts to compile both the shared PQC provider (`autheo-pqc-core::liboqs`) and this standalone crate with the exact same algorithm selection.
- `autheo-mldsa-dilithium` and `autheo-mldsa-falcon` now share the same structure (types module + `liboqs` + deterministic fallback), so Kyber/Dilithium/Falcon packaging is symmetrical when the repos are split out.

## Testing

- `cargo test -p autheo-mlkem-kyber` runs deterministic unit tests.
- `cargo test -p autheo-mlkem-kyber --features liboqs -- --ignored` exercises the liboqs-backed round trip (only available on native targets with `liboqs` installed).

### Windows toolchains (MSVC)

The MSVC target does **not** link `Advapi32` automatically, yet `liboqs`
requires the Windows CryptoAPI entry points. The workspace-level
`.cargo/config.toml` now contains:

```
[target.'cfg(windows)']
# liboqs pulls in CryptoAPI symbols; Advapi32 satisfies them on MSVC.
rustflags = ["-ladvapi32"]
```

so Kyber builds/tests that enable `liboqs` link the required system library
without extra flags. If you override `RUSTFLAGS`, append `-ladvapi32` or MSVC
will raise `LNK2019` errors when running
`cargo test -p autheo-mlkem-kyber --features liboqs -- --ignored`.
