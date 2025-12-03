# Autheo PrivacyNet

PrivacyNet is the orchestration layer that braids differential privacy, CKKS/BFV-style FHE jobs, zk proof attestation, and the 5D EZPH manifold anchored inside Chronosync. It exposes a deterministic engine (`PrivacyNetEngine`) plus a JSON-RPC surface so tenants can submit DP queries while inheriting the same telemetry and anchoring guarantees as Autheo PQCNet.

## What it provides

- **DP + budget governance** &mdash; `PrivacyBudgetLedger` tracks per-session eps/delta budgets before a DP query is run.
- **Deterministic chaos** &mdash; `ChaosOracle` injects Lorenz/Chua guidance into DP/FHE noise so repeated runs converge to the same envelope for auditing.
- **Homomorphic compute** &mdash; `FheLayer` wraps CKKS-scale slots or mock evaluators before ciphertexts are fed into EZPH manifolds.
- **EZPH anchoring** &mdash; Every request becomes an `EzphRequest` that is anchored via `autheo-pqcnet-5dezph`, yielding Chronosync receipts and privacy proof metadata.

## How it works

1. **Ingress** &mdash; RPC payloads are wrapped in `RpcEnvelope` and validated against `ApiConfig` limits.
2. **Budget reservation** &mdash; `PrivacyBudgetLedger::claim` enforces `session_epsilon`, `session_delta`, and query counts before a DP mechanism executes.
3. **Chaos + DP execution** &mdash; `ChaosOracle` samples a deterministic attractor trajectory per request seed and `DifferentialPrivacyEngine` runs the configured mechanism (Gaussian/Laplace, etc.) against the tenant query.
4. **Homomorphic prep** &mdash; Slots from the request go through `FheLayer::execute`, returning a ciphertext ready for manifold projection.
5. **EZPH entanglement** &mdash; `DefaultEzphPipeline::entangle_and_anchor` produces the manifold state, CKKS metadata, zk proof, and Chronosync vertex receipt.
6. **Response assembly** &mdash; `PrivacyEnhancedIcosuple::assemble` fuses DP samples, chaos traces, budget claims, and EZPH metadata so tenants can verify that every privacy guardrail was honored.

## Code flow diagram

```mermaid
flowchart LR
    RPC[JSON-RPC request] --> Validate[API validation]
    Validate --> Budget{Budget ledger}
    Budget -->|claim OK| Chaos[Chaos oracle]
    Chaos --> DP[DP engine]
    Chaos --> FHE[FHE layer]
    DP --> Assemble[PrivacyEnhancedIcosuple]
    FHE --> EZPH[EZPH pipeline]
    EZPH --> Chronosync[HypergraphModule.apply_anchor_edge]
    Chronosync --> Assemble
    Assemble --> Reply[PrivacyNetResponse]
```

## Key modules

- `src/pipeline.rs` &mdash; Owns `PrivacyNetEngine`, budget enforcement, and hand-off to the EZPH pipeline.
- `src/dp.rs` &mdash; Defines `DifferentialPrivacyEngine`, supported mechanisms, and seed derivation.
- `src/fhe.rs` &mdash; Lightweight CKKS/BFV mock interface plus `HomomorphicJob` types.
- `src/chaos.rs` &mdash; Lorenz/Chua attractor sampling that keeps DP noise deterministic per request seed.
- `src/api.rs` &mdash; JSON-RPC envelope plus RPC verbs (DP query, chaos perturbation, FHE compute, trait proofs, budget composition).

## Configuration knobs

`PrivacyNetConfig` bundles every subsystem:

- `ezph` &mdash; `EzphConfig` forwarded to `autheo-pqcnet-5dezph` (`privacy.max_entropy_leak_bits`, `manifold.projection_rank`, etc.).
- `dp` &mdash; Mechanism selection, sigma/epsilon defaults, and RNG seeds.
- `fhe` &mdash; Slot counts, multiplicative depth, bootstrap period for the circuit runner.
- `budget` &mdash; Session epsilon/delta guardrails and max query count per tenant.
- `chaos` &mdash; Lorenz/Chua parameters controlling deterministic perturbations.
- `api` &mdash; Payload limits and public input ceilings for the RPC surface.

## How to test it

```bash
# Run the walkthrough example (shows DP + EZPH anchoring end-to-end)
cargo run -p autheo-privacynet --example privacynet_walkthrough

# Execute the crate's full test suite
cargo test -p autheo-privacynet

# Focused integration test that exercises DP -> EZPH wiring
cargo test -p autheo-privacynet tests::privacynet
```

The walkthrough prints the chaos sample, DP sample, privacy budget claim, and the resulting EZPH receipt so you can verify every stage without real CKKS/ZK engines.

## Extending / integrating

- **Bring your own ZK or FHE** by implementing the traits exported from `autheo-pqcnet-5dezph` and wiring them into `PrivacyNetConfig::ezph` and `FheLayerConfig`.
- **Swap DP mechanisms** via `DpQuery`/`DpMechanism`; the engine auto-derives noise from the seeded attractor state.
- **Observe privacy telemetry** by logging the `PrivacyNetResponse.privacy_report` plus the `DpQueryResult.zk_proof_digest` to prove which trait bound was enforced per query.
