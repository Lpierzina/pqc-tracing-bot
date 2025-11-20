 # autheo-pqcnet-5dqeh
 
 `autheo-pqcnet-5dqeh` packages the Five-Dimensional Qubit-Enhanced Hypergraph (5D-QEH) concept into a Rust module so it can graduate into its own repo when the Autheo-One roadmap demands it. The crate distills the primer below into tangible code: 4096-byte icosuples, temporal-weighted entanglement, pulsed laser propagation, and crystalline storage tiers that keep TupleChain, QS-DAG, and AI overlays in lockstep.
 
 ## Design Principles · 5D Hypergraph vs Classical DAGs
 
 - **Five-dimensional manifold** – vertices inhabit (x, y, z, t, q) space where `q` encodes superposition/entanglement metadata. The crate’s `HypergraphState` and `TemporalWeightModel` capture that extra axis as ANN similarity + TW scoring.
 - **Icosuple-first state** – each vertex is a 4096-byte icosuple bundling hashed payloads, multi-generational PQC (Kyber KEM + Dilithium/Falcon signatures), and 2048-dimensional (simulated) embeddings. The `Icosuple::synthesize` helper mirrors the icosuple build pipeline.
 - **Temporal Weight (TW) voting** – Lamport-style clocks blend with QRNG entropy, parent coherence, and ANN similarity so vertices entangle up to 100 parents without diverging. See `TemporalWeightModel::score` in `src/lib.rs`.
 - **Crystalline tiering** – anything exceeding payload/time/ANN thresholds is offloaded to Layer-0 crystalline storage, reflecting the femtosecond laser archival process described in the primer.
 
 ## Architecture in Brief
 
 | Layer | What happens in code | Primer reference |
 | --- | --- | --- |
 | Layer-0 Crystalline | `StorageTarget::Crystalline` toggles when icosuples exceed time/size/ANN guardrails; archival counts bubble up through `SimulationReport`. | Femtosecond laser voxels (360 TB/mm³) + delta compression (reducing 540 PB/h to 54 PB). |
 | Layer-1 QS-DAG evolution | `HypergraphState::insert` enforces ≤100 parents and replays TW scoring before persisting vertices. | Tuplechain → icosuple tiers with Lamport/TW ordering and hybrid PoS + TW consensus. |
 | Layer-2 Laser Mesh | `FiveDqehSim::emit_laser_paths` emits pulsed channels (1 Gbps–1 Tbps each) with QKD flags, echoing laser-pulsed paths and CHSH-driven sharding. | Pulsed femtosecond lasers + QKD overlays (≤10 ps intra-shard sync). |
 | Layer-3 THEO AI overlay | Simulator intents mimic THEO agents driving entanglement decisions (`SimulationIntent::entangle`), preparing hooks for THEO/QVM validation. | AI agents, Grapplang DSL, and THEO QVM formal verification. |
 
 ## Integrations Across Autheo-One
 
 - **AutheoID / SSI** – icosuples can anchor verifiable credentials; ANN similarity stands in for zero-knowledge disclosure windows per AIP-1/AIP-12.
 - **DevHub + Grapplang** – the crate’s public API (`HypergraphState`, `FiveDqehSim`, `SimulationIntent`) lines up with the DevHub SDK model so AI agents or CLI tooling can submit intents like “entangle transaction X with Y under Z constraints.”
 - **DePIN / Entropy nodes** – `SimulationIntent::entangle` accepts `qrng_entropy_bits`, allowing entropy feeds from hardware RNGs or entropy pools.
 - **RPCNet bridges** – `VertexReceipt` exposes storage placement so relay stacks know when to stream data over Dilithium-secured bridges into external DAGs or EVM shards.
 
 ## Sequence Diagram · How 5D-QEH Flows
 
 ```mermaid
 sequenceDiagram
     participant Agent as THEO Agent / dApp
     participant API as 5D-QEH API (`SimulationIntent`)
     participant Hypergraph as Hypergraph Engine
     participant Laser as Pulsed Laser Mesh
     participant Crystal as Crystalline Layer
 
     Agent->>API: Build 4096B icosuple + entanglement request
     API->>Hypergraph: TW score + ANN check (≤100 parents)
     Hypergraph-->>Laser: Derive QKD-enabled pulse plan
     Laser-->>Hypergraph: Gossip confirmations (<10 ps)
     Hypergraph->>Crystal: Offload high-mass / stale tuples
     Hypergraph-->>Agent: Receipt + tier placement
 ```
 
 ## Demo / Simulation
 
 1. Run the coherence walkthrough example to simulate a small epoch with laser telemetry:
    ```bash
    cargo run -p autheo-pqcnet-5dqeh --example coherence_walkthrough
    ```
    You’ll see how many vertices were accepted, how many were pushed into crystalline storage, and the first few laser channels (Gbps, latency in picoseconds, QKD flag).
 2. Embed the library into notebooks or sentry prototypes by wiring `FiveDqehSim`, `HypergraphState`, and `SimulationIntent::entangle` the same way the example does.
 
 ## Tests
 
 - Unit tests live alongside the library (`src/lib.rs`): `cargo test -p autheo-pqcnet-5dqeh`.
 - The suite covers TW scoring, parent-limit enforcement, and simulator telemetry to ensure regressions are caught before this module spins out into its own repository.
 
 ## When You Need More
 
 The primer sections (“Design Principles”, “Architectural Components”, “Integrations”, “Interfacing Mechanisms”, “Performance & Security”, “Broader Implications”) are intentionally mirrored in the API:
 
 - `TemporalWeightModel` ↔ time-weighted voting (TW accrual + QRNG entropy)
 - `HypergraphState` ↔ tuplechain / icosuple tiers with ANN-driven parent capping
 - `LaserPath` + `SimulationReport` ↔ pulsed laser propagation + trillion TPS telemetry
 - `StorageTarget` ↔ crystalline vs hot tiers for DePIN, SCIF, and mission data
 
 Use this crate as the nucleus for the eventual stand-alone 5D-QEH repo—contributions here will port cleanly once the Autheo-One mono-repo splits. 
