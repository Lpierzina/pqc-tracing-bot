# autheo-pqcnet-hqc

`autheo-pqcnet-hqc` provides **production-only HQC (Hamming Quasi-Cyclic) KEM** bindings for the PQCNet stack. It mirrors the packaging we use for Kyber/Dilithium/Sphincs, but intentionally ships *only* the hardened `liboqs` implementation so HQC can serve as a real backup if ML-KEM/Kyber is ever at risk.

> ⚠️ There are **no deterministic shims or mock engines** in this crate. If `liboqs` is unavailable, the build intentionally fails so we never ship a simulated HQC path.

## Why HQC in PQCNet

- **Backup for ML-KEM** – HQC is on NIST's on-ramp as the alternate IND-CCA2 KEM. Wiring it into PQCNet means `autheo-pqc-core` can rotate to HQC if Kyber is patched or under review.
- **Drop-in adapter** – The API matches the other PQC engines, so host runtimes can register `HqcLibOqs` with `autheo_pqc_core::kem::MlKemEngine` without touching the handshake logic or WASM ABI.
- **Full liboqs fidelity** – We pull the HQC implementation directly from `oqs::kem::Algorithm::{Hqc128,Hqc192,Hqc256}` and expose every byte untouched so auditors can compare transcripts between PQCNet and standalone liboqs harnesses.

## Feature matrix

| Feature | Default? | Purpose |
| --- | --- | --- |
| `liboqs` | ✅ | Enables the only available implementation (via `oqs` + `liboqs`). Disabling it triggers a compile error. |

## Usage

```rust
use autheo_pqcnet_hqc::{HqcAlgorithm, HqcLibOqs};

let engine = HqcLibOqs::new(HqcAlgorithm::Hqc256);
let keypair = engine.keypair()?;
let encapsulation = engine.encapsulate(&keypair.public_key)?;
let shared = engine.decapsulate(&keypair.secret_key, &encapsulation.ciphertext)?;
assert_eq!(shared, encapsulation.shared_secret);
```

### Wiring HQC into `autheo-pqc-core`

```rust
use autheo_pqc_core::kem::{MlKem, MlKemEngine, MlKemKeyPair};
use autheo_pqc_core::types::SecurityLevel;
use autheo_pqcnet_hqc::{HqcAlgorithm, HqcLibOqs, HqcResult};

struct HqcKem(HqcLibOqs);
impl MlKem for HqcKem {
    fn level(&self) -> SecurityLevel {
        match self.0.level() {
            autheo_pqcnet_hqc::HqcLevel::Hqc128 => SecurityLevel::MlKem128,
            autheo_pqcnet_hqc::HqcLevel::Hqc192 => SecurityLevel::MlKem192,
            autheo_pqcnet_hqc::HqcLevel::Hqc256 => SecurityLevel::MlKem256,
        }
    }

    fn keygen(&self) -> autheo_pqc_core::error::PqcResult<MlKemKeyPair> {
        let pair = self.0.keypair()?;
        Ok(MlKemKeyPair {
            public_key: pair.public_key,
            secret_key: pair.secret_key,
            level: self.level(),
        })
    }

    // delegate encapsulate/decapsulate to HqcLibOqs ...
}

let engine = MlKemEngine::new(HqcKem(HqcLibOqs::new(HqcAlgorithm::Hqc192)));
```

Use that engine when instantiating `autheo_pqc_core::key_manager::KeyManager` so `pqc_handshake` can encapsulate with HQC while Kyber rotations are paused. Because the API matches Kyber's adapter, the same QFKH, QSTP, and TupleChain paths continue to work.

### Backup testing flow

1. **Build PQC core with liboqs**
   ```bash
   cargo build -p autheo-pqc-core --features liboqs
   ```
2. **Run the HQC handshakes**
   - inject `HqcLibOqs` via the `MlKemEngine` snippet above
   - call `cargo run -p autheo-pqc-core --example handshake_demo` to record HQC-based transcripts
3. **Verify envelopes in wazero**
   - rebuild `autheo-pqc-wasm` with the HQC-enabled core
   - run `go run ./wazero-harness -wasm target/wasm32-unknown-unknown/release/autheo_pqc_wasm.wasm --kem hqc256`
4. **Failover drills**
   - set `PQCNET_BACKUP_KEM=hqc256` in the QSTP/relayer configs to force HQC tunnels while Kyber is patched

### Wiring into Kyber failover controls

`autheo-pqc-core::liboqs::LibOqsProvider` exposes `HqcFallbackConfig` so this crate can act as the automatic Kyber backup. Typical flow:

```rust
use autheo_pqc_core::liboqs::{HqcFallbackConfig, LibOqsConfig, LibOqsProvider};
use autheo_pqcnet_hqc::HqcLevel;

let mut cfg = LibOqsConfig::default();
cfg.hqc_backup = Some(HqcFallbackConfig {
    level: HqcLevel::Hqc256,
    auto_failover: true,
});
let provider = LibOqsProvider::new(cfg)?;

// Flip to HQC when ops detects a Kyber issue.
provider.force_hqc_backup();
```

`is_using_hqc_backup()` reports the current mode so telemetry and sentries can attest when HQC handled production traffic, and `use_kyber_primary()` reverts to the Kyber engine as soon as maintenance windows close.

## Testing

The crate only exposes liboqs tests, so running them proves the real HQC implementation succeeds end-to-end:

```bash
cargo test -p autheo-pqcnet-hqc

# Validate the Kyber→HQC failover inside the core provider
cargo test -p autheo-pqc-core --features liboqs \
  liboqs::tests::kyber_failover_switches_to_hqc_backup
```

All tests perform `keypair → encapsulate → decapsulate` using the selected HQC parameter set. There are no ignored or mocked tests; failures map directly to `liboqs` errors.

## Operational guidance

- Enable HQC in PQCNet configs (relayers, sentries, DW3B overlay) by advertising both Kyber and HQC public keys, then mark HQC as `backup_only = true`. The runtime will prefer Kyber but can flip to HQC whenever `pqcnet-qace` flags a lattice risk.
- Record HQC telemetry in `pqcnet-telemetry` so Chronosync / DW3B regulators can see which tunnels ran on HQC during drills.
- Pair HQC with `autheo-pqcnet-sphincs` dual signatures to keep both KEM and signature redundancy when Kyber remediation windows occur.
