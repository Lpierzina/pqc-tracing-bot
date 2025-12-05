# autheo-pqcnet-5dezph

`autheo-pqcnet-5dezph` layers the **Five-Dimensional Entangled Zero-Knowledge Privacy
Hypergraph (5D-EZPH)** on top of Autheo's `autheo-pqcnet-5dqeh` module. It fuses chaos-driven
manifold synthesis, CKKS-style homomorphic aggregation, and ZK proof metadata so PrivacyNet can
anchor quantum privacy overlays directly inside Chronosync.

## Highlights

- **Manifold synthesis** – builds 5D manifolds where dimensions encode spatial routing, temporal
  noise, quantum entropy pools, chaotic perturbations, and homomorphic layers.
- **Chaos primitives** – deterministic Lorenz + Chua attractors plus a logistic map feed the privacy
  manifold and laser telemetry, matching the AUTHEO PRIMER spec.
- **ZK/FHE bridges** – Halo2 proving + TFHE slot encryption ship as the default `DefaultEzphPipeline`
  backend, while the legacy mock evaluators remain available for deterministic testing.
- **Privacy metrics** – computes Rényi divergences, hockey-stick deltas, and privacy amplification
  bounds (< `1e-154`) before accepting a vertex; violations abort the anchor.
- **Projection helpers** – deterministic 5D→3D projections keep Pi-class nodes honest about
  dimensional reduction while exposing axis magnitudes for telemetry.

## How it works

1. **Seed derivation** – `derive_seed` hashes `EzphRequest` metadata (label, Lamport clock, tuple commitment, FHE slots) so chaos, DP, and proof engines stay in sync.
2. **Chaos + manifold build** – `LorenzChuaChaos::sample` produces attractor coordinates that drive `EzphManifoldState::build`, yielding homomorphic amplitude, spatial axes, and temporal noise.
3. **FHE and ZK prep** – `FheEvaluator::encrypt` turns the requested slots into ciphertexts while `ZkProver::prove` emits a proof bound to the same seed and public inputs.
4. **Icosuple construction** – `build_icosuple` encodes CKKS scale, quantum coordinates, PQC layer metadata, and entanglement coefficients into the payload that Chronosync expects.
5. **Anchoring** – `EzphPipeline::entangle_and_anchor` calls `HypergraphModule::apply_anchor_edge`, validates privacy via `evaluate_privacy`, and projects the manifold with `project_dimensions`.
6. **Outcome telemetry** – The returned `EzphOutcome` bundles the vertex receipt, privacy report, ZK digest, ciphertext, and projection data so host services can audit every stage.

## Code flow diagram

```mermaid
sequenceDiagram
    participant Host as Host service
    participant Pipeline as EzphPipeline
    participant Chaos as ChaosEngine
    participant FHE as FheEvaluator
    participant ZK as ZkProver
    participant Manifold as EzphManifoldState
    participant Chrono as HypergraphModule

    Host->>Pipeline: EzphRequest
    Pipeline->>Chaos: sample(seed)
    Chaos-->>Pipeline: chaos sample
    Pipeline->>Manifold: build(manifold cfg, chaos, slots)
    Pipeline->>FHE: encrypt(fhe_slots)
    FHE-->>Pipeline: FheCiphertext
    Pipeline->>ZK: prove(statement)
    ZK-->>Pipeline: ZkProof
    Pipeline->>Chrono: apply_anchor_edge(icosuple)
    Chrono-->>Pipeline: VertexReceipt
    Pipeline-->>Host: EzphOutcome
```

## Usage

```bash
# Run the reference walkthrough
cargo run -p autheo-pqcnet-5dezph --example ezph_walkthrough
```

The example instantiates a `HypergraphModule`, drives the EZPH pipeline, prints the anchored vertex
ID, temporal-weight score, privacy bounds, and the projected axes that Pi overlays consume.

## Testing

```bash
# Full crate tests (unit + integration)
cargo test -p autheo-pqcnet-5dezph

# Focus just on the pipeline integration test
cargo test -p autheo-pqcnet-5dezph pipeline
```

The `tests/pipeline.rs` suite now treats the Halo2/TFHE path as a **heavy** test so that
`cargo test` stays responsive in CI: export `RUN_HEAVY_EZPH=1` (or the broader
`RUN_HEAVY_ZK=1`) or build with `--features real_zk` when you want the full pipeline run.
Without one of those switches the test prints a skip message instead of hanging on the prover.

The Halo2 prover now persists its parameters + pinned verifying metadata under
`config/crypto/halo2.{params,pk,vk}` the first time it runs. The next invocation reuses those files
and refuses to proceed if the on-disk Powers-of-Tau (`k`) no longer matches the requested soundness.
When you opt into the heavy suite, cap Rayon’s global thread-pool so the host doesn’t try to spawn
128 workers for a single proof:

```
RUST_TEST_THREADS=1 RAYON_NUM_THREADS=1 RUN_HEAVY_EZPH=1 \
  cargo test -p autheo-pqcnet-5dezph --features real_zk
```

The prover now installs a single-threaded global Rayon pool automatically (unless
you set `RAYON_NUM_THREADS` or `AUTHEO_RAYON_THREADS`), so the heavy path no longer deadlocks when
Halo2 tries to spawn 128 workers on a cramped CI host.

When enabled, the suite stands up a `HypergraphModule`, runs
`DefaultEzphPipeline::entangle_and_anchor` with `EzphRequest::demo`, and asserts that a
vertex is anchored only when `EzphOutcome::privacy.satisfied` remains true. The walkthrough
example doubles as a manual test by printing the privacy metrics and icosuple payloads
that Chronosync would ingest.

## Integration points

- **Rust hosts** embed [`DefaultEzphPipeline`](src/pipeline.rs) and feed it `EzphRequest` structs
  sourced from TupleChain, AutheoID, or AIPP overlays. Toggle `EzphConfig::zk_prover` /
  `EzphConfig::fhe_evaluator` to switch between the Halo2/TFHE backends and the mock pipeline.
- **Python quantum harness** (`quantum/ezph_pipeline.py`) mirrors the same chaos + QuTiP stack for
  lab validation, taking the JSON emitted by `pqcnet-qs-dag` examples.
- **Telemetry** can record `EzphOutcome::privacy` to prove that every anchored vertex satisfied the
  advertised `ε < 0.01`, `δ ≈ 0` bounds even under Lorenz/Chua chaos injection.

Swap `MockCircomProver` / `MockCkksEvaluator` for production engines by flipping
`EzphConfig::zk_prover` / `EzphConfig::fhe_evaluator`. The Halo2 + TFHE implementation now backs the
default pipeline, and additional backends can keep using the same `ZkProver`/`FheEvaluator` traits so
the pipeline automatically threads the proofs/ciphertexts into the icosuple payloads and PQC
signatures.
