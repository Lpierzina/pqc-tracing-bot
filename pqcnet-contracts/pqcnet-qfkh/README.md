# pqcnet-qfkh

Quantum-Forward Key Hopping (QFKH) extends PQCNet with an epoch-based key
rotation controller. It deterministically hops ML-KEM key pairs, derives fresh
symmetric material for every epoch, and records verifiable commitments so that
captured frames stay indecipherable to future quantum adversaries.

## Quick start (Rust)

```rust
use autheo_pqc_core::adapters::DemoMlKem;
use autheo_pqc_core::kem::MlKemEngine;
use pqcnet_qfkh::{QfkhConfig, QuantumForwardKeyHopper};

let config = QfkhConfig::new(5_000, 2)?; // hop every 5 seconds, keep 2 epochs ahead
let mut responder = QuantumForwardKeyHopper::new(MlKemEngine::new(Box::new(DemoMlKem::new())), config);
let mut initiator = QuantumForwardKeyHopper::new(MlKemEngine::new(Box::new(DemoMlKem::new())), config);

let ticket = responder.announce_epoch(6_500)?; // expose public ticket for epoch 1
let (capsule, initiator_session) = initiator.encapsulate_for(&ticket, 7_000)?;
let responder_session = responder.activate_from(&capsule, 7_000)?;
assert_eq!(initiator_session.derived_key, responder_session.derived_key);
```

## Demos & sims

- Run the deterministic simulator to observe live key hopping:
  - `cargo run -p pqcnet-qfkh --example qfkh_sim`
  - The example prints each epoch announcement, the derived commitments, and the
    derived symmetric key fingerprint so you can trace the forward secrecy
    envelope end-to-end.
- `examples/qfkh_sim.rs` showcases how to wire `ensure_lookahead`,
  `announce_epoch`, and `encapsulate_for` to build a long-lived rotating session
  without touching any networking stacks.

## Tests

- Library + integration tests: `cargo test -p pqcnet-qfkh`
  - Exercises shared-secret agreement, enforcement of hop windows, and
    lookahead materialization logic.
- CI can target the same command; tests reset the deterministic ML-KEM fixture
  via `autheo_pqc_core::runtime::reset_state_for_tests()` so they are repeatable
  in WASM and native hosts.

## Sequence diagram

```mermaid
sequenceDiagram
    autonumber
    participant Scheduler as Responder (`announce_epoch`)
    participant Initiator as Initiator (`encapsulate_for`)
    participant Network as Transport
    participant Responder as Responder (`activate_from`)
    participant Policy as Hopper State (`needs_rotation`)

    Scheduler->>Initiator: Ticket = `QuantumForwardKeyHopper::announce_epoch()`
    Initiator->>Initiator: `encapsulate_for(ticket)` derives hop key & capsule
    Initiator->>Network: Publish `QfkhHopCiphertext`
    Network-->>Responder: Capsule delivery
    Responder->>Responder: `activate_from(capsule)` decapsulates & verifies commitment
    Responder->>Policy: Update `active_session` and check `needs_rotation`
```

## Future split-ready layout

The crate mirrors other PQCNet repos (`pqcnet-qstp`, `pqcnet-qs-dag`, etc.) so it
can be lifted into its own repository without restructuring. Public APIs are
self-contained and depend only on `autheo-pqc-core` plus `blake2`/`digest`.
