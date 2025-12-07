# autheo-pqcnet-sphincs

`autheo-pqcnet-sphincs` packages **FIPS 205 – SLH-DSA SPHINCS+** signatures for the PQCNet suite. It mirrors the pattern we already use for Kyber, Dilithium, and Falcon: a reproducible deterministic engine for `no_std`/`wasm32` targets and a liboqs-backed implementation for production validators, relayers, and DW3B overlay nodes. This crate is the fourth production-grade signature source that `autheo-pqc-core` can compose, giving us a hash-based fallback whenever lattice engines are constrained or under review.

## Why SPHINCS+ in PQCNet

- **FIPS 205 coverage** – Implements the SHAKE-based SLH-DSA parameter sets (`128s/f`, `192s/f`, `256s/f`) exactly as specified in FIPS 205 (Dec 2023).
- **Hash-based redundancy** – SPHINCS+ is stateless and hash-based, so we can survive hypothetic lattice breaks without rewriting the PQC handshake logic.
- **Nested signature flows** – `autheo-pqc-core::signatures::SignatureManager` now receives the same API surface from Dilithium, Falcon, and SPHINCS+. Hosts can dual-sign every transcript (`Dilithium || SPHINCS+`) before the TupleChain receipt is emitted, or dedicate SPHINCS+ to watcher quorums / chaos routes (`pqcnet-qace`).
- **Production only** – The deterministic engine exists purely for `wasm32` demos; the README, examples, and tests assume `liboqs` is the default for native builds.

## Feature matrix

| Feature | Default? | Purpose |
| --- | --- | --- |
| `deterministic` | ✅ | Reproducible, `no_std` fallback for `autheo-pqc-wasm` and CI fixtures. |
| `liboqs` | ⛔ (opt-in) | Links `oqs::sig::Algorithm::SphincsShake*` so validators use real SPHINCS+. |
| `std` | Pulled in by `liboqs` | Enables `std` so liboqs can allocate + link OpenSSL. |

## Usage

```rust
use autheo_pqcnet_sphincs::{
    SphincsPlusDeterministic, SphincsPlusSecurityLevel,
};

let engine = SphincsPlusDeterministic::new(SphincsPlusSecurityLevel::Shake128s);
let pair = engine.keypair().expect("keypair");
let sig = engine.sign(&pair.secret_key, b"pqcnet chaos proof").expect("sign");
engine.verify(&pair.public_key, b"pqcnet chaos proof", &sig).expect("verify");
```

Native deployments flip to the liboqs engine:

```rust
use autheo_pqcnet_sphincs::{SphincsPlusLibOqs, SphincsPlusSecurityLevel};

autheo_pqcnet_sphincs::oqs::init();
let engine = SphincsPlusLibOqs::new(SphincsPlusSecurityLevel::Shake256s);
let pair = engine.keypair().expect("keypair");
let sig = engine.sign(&pair.secret_key, b"tuple receipt").expect("sign");
engine.verify(&pair.public_key, b"tuple receipt", &sig).expect("verify");
```

## Working with `autheo-pqc-core`

1. **Wire into `MlDsa`** – Wrap either engine in `MlDsa` so `autheo_pqc_core::signatures::SignatureManager` can dual-register keys:
   ```rust
   use autheo_pqc_core::dsa::{MlDsa, MlDsaEngine, MlDsaKeyPair};
   use autheo_pqc_core::types::SecurityLevel;
   use autheo_pqcnet_sphincs::{SphincsPlusLibOqs, SphincsPlusSecurityLevel};

   struct HashDsa(SphincsPlusLibOqs);
   impl MlDsa for HashDsa {
       fn level(&self) -> SecurityLevel { SecurityLevel::MlDsa256 }
       fn keygen(&self) -> PqcResult<MlDsaKeyPair> {
           let pair = self.0.keypair()?;
           Ok(MlDsaKeyPair {
               public_key: pair.public_key,
               secret_key: pair.secret_key,
               level: SecurityLevel::MlDsa256,
           })
       }
       fn sign(&self, sk: &[u8], msg: &[u8]) -> PqcResult<Bytes> {
           self.0.sign(sk, msg).map_err(Into::into)
       }
       fn verify(&self, pk: &[u8], msg: &[u8], sig: &[u8]) -> PqcResult<()> {
           self.0.verify(pk, msg, sig).map_err(Into::into)
       }
   }

   let hash_engine = MlDsaEngine::new(Box::new(HashDsa(
       SphincsPlusLibOqs::new(SphincsPlusSecurityLevel::Shake256s),
   )));
   ```
   Register it alongside the existing Dilithium/Falcon engine to force SPHINCS+ co-signatures for every QSTP tunnel and TupleChain receipt.
2. **Nested signatures** – Chronicle nodes can call `SignatureManager::sign_kem_transcript` twice (Dilithium + SPHINCS+) and persist both signatures inside `pqcnet-qs-dag::AnchorEvidence`. The merkleized chaos routes (`pqcnet-qace`) can then require both signatures before mutating a route.
3. **DW3B / PrivacyNet hooks** – `autheo-dw3b-overlay` and `autheo-privacynet` can treat SPHINCS+ as the “chaos proof” layer: include the SPHINCS+ signature inside every EZPH receipt so privacy regulators can audit a hash-based trail if ML-DSA is paused.

## PQCNet + PQC Core flow

```
autheo-pqcnet-sphincs
    ↓ (liboqs Sig::new)
SphincsPlusLibOqs
    ↓
autheo_pqc_core::dsa::MlDsaEngine
    ↓
SignatureManager::generate_signing_key()
    ↓
pqcnet-qstp / pqcnet-qs-dag / TupleChain receipts
```

- `wazero-harness` can now request `--hash-signatures` to force SPHINCS+ transcripts before the `QsDagPqc::verify_and_anchor` step.
- The chaos + redundancy path Ken requested is implemented by anchoring both Dilithium and SPHINCS+ signatures inside Chronosync TW scoring so watchers can slash routes that lose either signature.

## Testing

```
# Deterministic fallback (wasm / CI)
cargo test -p autheo-pqcnet-sphincs

# Production liboqs wiring (native only)
cargo test -p autheo-pqcnet-sphincs --features liboqs -- --ignored
```

Both suites cover keygen, sign/verify, tamper detection, and the `SignatureManager` integration snippet above. No simulators or mock KATs are needed—the liboqs tests call the exact SPHINCS+ reference that Autheo deploys on validators.*** End Patch
