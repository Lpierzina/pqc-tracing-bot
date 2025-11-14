# PQCNet Contracts

**PQCNet** is a Rust contract library for NIST-compliant post-quantum cryptography, designed to plug into Autheo-One’s PQCNet node and QS-DAG consensus.

It provides:

- **ML-KEM (Kyber)** for key encapsulation and session key establishment  
- **ML-DSA (Dilithium)** for digital signatures and batch verification  
- **Rotating, threshold-protected KEM key management** (e.g. *t* = 3, *n* = 5)  
- **Atomic sign-and-exchange flows** for securing key exchanges  
- **QS-DAG integration hooks** for anchoring PQC signatures in the DAG

> ⚠️ This crate does **not** implement crypto primitives itself.  
> It defines **traits and contract logic** around *audited, NIST-compliant* PQC engines that you plug in from `autheo-pqc` (Kyber/Dilithium WASM, PQClean, etc.).

---

## High-Level Architecture

### Modules

- `kem.rs` – ML-KEM abstraction (`MlKem`, `MlKemEngine`, `MlKemKeyPair`)
- `dsa.rs` – ML-DSA abstraction (`MlDsa`, `MlDsaEngine`, `MlDsaKeyPair`)
- `key_manager.rs` – rotating KEM key management and policy
- `signatures.rs` – signing, verifying, batch verify, and “sign KEM transcript”
- `qs_dag.rs` – QS-DAG integration shim for anchoring PQC signatures
- `types.rs` – common types (`KeyId`, `EdgeId`, `SecurityLevel`, etc.)
- `error.rs` – standardized error type (`PqcError`) and `PqcResult<T>`

All modules are written to be `no_std`-friendly and can be compiled to WASM for embedding in Autheo’s PQCNet node.

---

## Design Goals

### 1. NIST-Compliant PQC

PQCNet is built around NIST standards:

- **ML-KEM** (Kyber) – FIPS 203  
- **ML-DSA** (Dilithium) – FIPS 204  

The contract layer:

- **Assumes** an underlying engine that:
  - Passes NIST Known Answer Tests (KATs)
  - Implements **IND-CCA2** security for ML-KEM
  - Implements **EUF-CMA** security for ML-DSA
  - Is constant-time to avoid timing side channels
- Exposes safe, typed APIs for:
  - Key generation
  - Encapsulation / decapsulation
  - Signing / verifying / batch verifying

All heavy crypto is handled by the **PQC engines**, not this repo.

---

### 2. Rotating Key Management (ML-KEM)

`key_manager.rs` manages an active KEM key and a rotation policy:

- Generates a new ML-KEM keypair via `MlKemEngine`
- Derives a stable `KeyId` from the public key + timestamp
- Stores:
  - `public_key`
  - `created_at`
  - `expires_at`
  - `SecurityLevel`
- Enforces a **rotation interval** (e.g. `300_000 ms` = 300 s)
- Exposes an `encapsulate_for_current()` helper to derive fresh session keys

Threshold-sharing (e.g. Shamir with *t = 3*, *n = 5*) is treated as a **host responsibility**:

- The contract defines the **policy** (`ThresholdPolicy { t, n }`)
- The PQCNet / validator infra actually:
  - Splits the secret key into shares
  - Distributes & stores them in secure enclaves / services
  - Recombines shares when needed

This separation keeps the contract simple and allows richer math libraries and HSMs off-chain.

---

### 3. ML-DSA Signatures & Batch Verification

`signatures.rs` manages signing keys and signatures using `MlDsaEngine`:

- Generates ML-DSA keypairs and registers their `KeyId`
- Signs arbitrary messages with a secret key
- Verifies signatures by `KeyId`
- Provides **batch verification** for high throughput:
  - Designed to support ≥ 100 operations per batch
  - Can be swapped to use native aggregated verification if your engine supports it

It also implements a **combined flow**:

> *Sign the KEM transcript atomically* — e.g. signing a key exchange

- Takes an `MlKemEncapsulation` (ciphertext + shared secret) and a context
- Builds a deterministic transcript
- Signs it with ML-DSA in one call

This addresses the “signing a key exchange with no intermediate data exposure” requirement.

---

### 4. QS-DAG Integration

`qs_dag.rs` defines a `QsDagHost` trait that your consensus layer implements:

- `attach_pqc_signature(edge_id, signer, signature)`
- `get_edge_payload(edge_id)`

Then `QsDagPqc` provides a helper:

```rust
verify_and_anchor(
    edge_id,
    signer_key_id,
    signature,
    verify_fn, // e.g. SignatureManager::verify
)
The flow:

Load the DAG payload for the given edge_id.

Verify the ML-DSA signature over that payload.

On success, attach the signature to the DAG.

This is where you can benchmark PQC overhead compared to baseline and enforce the < 5% DAG edge update overhead target.

Example Usage

These examples use pseudo “host engines” – in real deployments you’d bind to WASM or native implementations from autheo-pqc.

Key Generation & Rotation (ML-KEM)
use pqcnet_contracts::kem::{MlKem, MlKemEngine};
use pqcnet_contracts::key_manager::{KeyManager, ThresholdPolicy};
use pqcnet_contracts::types::TimestampMs;

struct HostKemImpl; // your Kyber implementation

impl MlKem for HostKemImpl {
    // implement level(), keygen(), encapsulate(), decapsulate()
}

fn example_key_rotation(now_ms: TimestampMs) {
    let kem_engine = MlKemEngine::new(&HostKemImpl);

    let mut km = KeyManager::new(
        kem_engine,
        ThresholdPolicy { t: 3, n: 5 },
        300_000, // 300 seconds
    );

    let current = km.keygen_and_install(now_ms).unwrap();

    // Later…
    let _maybe_rotated = km.rotate_if_needed(now_ms + 301_000).unwrap();
}

Signing & Verifying (ML-DSA)
use pqcnet_contracts::dsa::{MlDsa, MlDsaEngine};
use pqcnet_contracts::signatures::SignatureManager;

struct HostDsaImpl; // your Dilithium implementation

impl MlDsa for HostDsaImpl {
    // implement level(), keygen(), sign(), verify()
}

fn example_signing() {
    let dsa_engine = MlDsaEngine::new(&HostDsaImpl);
    let mut sig_mgr = SignatureManager::new(dsa_engine);

    let now = 1_700_000_000_000u64;
    let (key_state, keypair) = sig_mgr.generate_signing_key(now).unwrap();

    let msg = b"hello, quantum world";
    let sig = sig_mgr.sign(&keypair.secret_key, msg).unwrap();

    // Verify by logical KeyId
    sig_mgr.verify(&key_state.id, msg, &sig).unwrap();
}

Performance Targets

The crate is designed to support:

Latency: < 1 ms per sign/verify operation on modern CPUs (e.g. Ryzen 9), assuming optimized engines

Throughput: ≥ 10,000 TPS via:

Batching (e.g. batch_verify)

Parallel execution for independent keys and DAG edges

Performance is primarily determined by:

The underlying ML-KEM / ML-DSA engine implementation

Host runtime (threading, SIMD, scheduling)

Storage and QS-DAG overhead

The contracts themselves are thin wrappers that do not introduce unnecessary allocations or complex control flow.

Security & Audit Notes

All cryptographic correctness and NIST guarantee proofs (IND-CCA2 / EUF-CMA) live in the PQC engine layer.

This repo’s responsibilities:

Avoid leaking intermediate secrets or partial outputs

Provide clean, minimal interfaces with clear semantics

Keep logic deterministic and side-channel-conscious

Recommended audit checks:

Verify that only approved PQC engines are wired into MlKemEngine / MlDsaEngine

Confirm no direct access to secret keys is exposed beyond the expected APIs

Confirm all key rotations and policies align with Autheo’s forward-secrecy requirements

Building & Testing
Build
# Standard
cargo build

# For WASM targets (example)
cargo build --target wasm32-unknown-unknown

Tests

Unit tests and integration tests should live alongside the engine bindings:

Crypto KAT tests → in the engine repo (e.g. autheo-pqc)

Contract logic tests → in this repo (tests/)

Example:

cargo test


You should include:

Key rotation tests (interval expiry, policy enforcement)

Threshold policy tests (t/n constraints)

Signature and batch-verification tests

DAG integration tests with a mocked QsDagHost

Roadmap

 Wire to Autheo’s Kyber/Dilithium WASM engines

 Add Shamir threshold helper for dev/test environments

 Add benchmarking harness for TPS & DAG overhead

 Expose FFI/ABI definitions for PQCNet node host (Go/Rust)

 Publish crate docs via cargo doc / hosted docs

---

## WASM Handshake ABI & Go wazero Harness

The crate now exports a minimal enclave surface so hosts can exercise it via
WASM:

- `pqc_alloc(len: u32) -> u32` / `pqc_free(ptr: u32, len: u32)` manage linear
  memory without exposing the allocator directly.
- `pqc_handshake(req_ptr, req_len, resp_ptr, resp_len) -> i32` consumes arbitrary
  request bytes and writes a fixed 32-byte digest to `resp_ptr`. Non-negative
  return values indicate the number of response bytes written; `-1` signals
  invalid input, `-2` indicates an undersized response buffer, and `-127`
  captures internal errors. See `pqcnet-contracts/src/handshake.rs` for the
  placeholder implementation.

### Build the WASM artifact

```
cd pqcnet-contracts
cargo build --release --target wasm32-unknown-unknown
# -> target/wasm32-unknown-unknown/release/pqcnet_contracts.wasm
```

### Run the Go+wazero harness

```
cd wazero-harness
go run . \
  -wasm ../pqcnet-contracts/target/wasm32-unknown-unknown/release/pqcnet_contracts.wasm
```

The harness (see `wazero-harness/main.go`) allocates request/response buffers
inside the module, invokes `pqc_handshake`, and prints the 32-byte digest in hex.
This is the starting point for wiring the actual Kyber/Dilithium engines and
embedding the enclave inside Autheo-One’s Go orchestrator.

### End-to-end flow

1. **Host request** – The Go harness builds a nonce-bearing string such as
   `client=autheo-demo&ts=<unix-nanos>` (see `buildRequestPayload`) and writes
   it into module memory using `pqc_alloc` + `Memory().Write`.
2. **WASM call** – Host code allocates a 64-byte response buffer, calls
   `pqc_handshake(req_ptr, req_len, resp_ptr, resp_len)`, and later frees both
   buffers via `pqc_free`.
3. **Contract logic** – Inside `pqc_handshake` (`src/wasm.rs`), the pointers are
   reinterpreted as Rust slices and passed to `handshake::execute_handshake`.
4. **Digest derivation** – `execute_handshake` prefixes the request with the
   domain separator `b"PQCNET_HANDSHAKE_V0"` and hashes it with BLAKE2s, copying
   the first 32 bytes into the caller-provided response slice. Any empty request
   or undersized buffer is rejected with `PqcError`.
5. **Result propagation** – Success returns the number of bytes written
   (`32`). Errors map to stable codes returned to the host:
   - `-1` → invalid input (null pointers / empty request);
   - `-2` → response buffer too small;
   - `-127` → catch-all internal error.
6. **Host output** – The harness reads the reported number of bytes back out of
   WASM memory, hex-encodes them, and logs both the request and deterministic
   response digest. Replaying the exact same request reproduces the same digest,
   which keeps the ABI stable while real ML-KEM/ML-DSA plumbing is brought
   online.

> ℹ️ Replacing the placeholder digest with a true PQC handshake simply means
> swapping `execute_handshake` for logic that:
> - pulls the current ML-KEM public key from `KeyManager`,
> - runs `encapsulate_for_current()` to derive a shared secret,
> - annotates/signs the transcript via `SignatureManager::sign_kem_transcript`,
> - serializes the resulting ciphertext, shared secret handle, and signature
>   back to the host. The wasm/Go ABI remains the same.

License

TBD – e.g. MIT / Apache-2.0 (align with Autheo’s policy).