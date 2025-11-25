# PQCNet Demo Progress Brief (Safe Company-Wide Share)

_Last updated: 2025-11-25_

This note gives you a company-wide-safe storyline to demo PQCNet progress from the past two weeks. It highlights what we can comfortably show live, which repo artifacts back up each claim, and what to keep redacted while still proving momentum.

## Purpose

- Equip you with a single script for the all-hands demo.
- Show real progress without disclosing sensitive key material, partner configs, or IP-heavy algorithms.
- Tie every talking point back to code so follow-up questions can be routed to the right crate.

## Guardrails

**Safe to share**
- High-level architecture (reuse `docs/pqcnet-architecture-integration.md` diagrams for context).
- Demo outputs generated from sample configs (`Config::sample()` et al.) and in-repo fixtures.
- Aggregate telemetry (counter names, flush cadence) without raw traces.

**Keep redacted**
- Real route policies, validator IDs, or prod key fingerprints.
- Any mention of partner meshes, node counts, or revenue KPIs.
- Internal tuning knobs (e.g., true GA heuristics, entropy sources) that do not surface in the examples below.

## Demo Story Arc (10–12 min)

1. **Opening:** Remind the room we now have a bundled PQCNode (Rust) that zer0veil can wrap. Point to the architecture doc for the “big picture.”
2. **Handshake Proof (autheo-pqc-core):** Run `cargo run -p autheo-pqc-core --example handshake_demo` to show ML-KEM + ML-DSA producing a PQC1 transcript, AES-GCM payload, and tuple key.
3. **Threshold & Rotation (autheo-pqc-core):** Follow with `cargo run -p autheo-pqc-core --example secret_sharing_demo` to demonstrate 2-of-3 bootstrap and 3-of-5 rotation with human-readable logs.
4. **Relayer Throughput (pqcnet-relayer):** `cargo run -p pqcnet-relayer --example pipeline` streams delivered/buffered counts plus telemetry snapshots, proving our queueing works.
5. **Control Plane Convergence (pqcnet-networking):** `cargo run -p pqcnet-networking --example control_plane` shows QS-DAG state sync across three simulated nodes.
6. **Watcher Quorum + Telemetry (pqcnet-sentry):** `cargo run -p pqcnet-sentry --example quorum_demo` highlights quorum thresholds and shared telemetry plumbing.
7. **Visual Touchpoint (WASM demo):** Load `autheo-mldsa-dilithium/wasm-demo/dilithium.html` locally to show the browser module that wraps ML-KEM/ML-DSA without exposing any prod endpoints.
8. **Close:** Reinforce that everything above runs with mock configs, so we keep IP safe while still shipping demonstrable progress.

## Two-Week Progress Snapshot

### Week 1 – Crypto Path Hardened

- **Handshake façade is locked in.** `autheo-pqc-core/src/handshake.rs` now rotates ML-KEM keys on demand, signs transcripts with ML-DSA, and serializes a PQC1 header + payload that demo logs can print. This is what powers the handshake demo.
- **Threshold rotation walkthrough.** The `secret_sharing_demo` example exercises `KeyManager`, `split_secret`, and quorum verification so we can narrate “2-of-3 bootstrap, rotate, reshare” live without touching real shards.
- **Key-management messaging.** The examples emit deterministic, anonymized key IDs so we can show health (“rotation happened”) without showing the actual key bytes.

### Week 2 – Network + Ops Instrumentation

- **Relayer pipeline telemetry.** `pqcnet-relayer/examples/pipeline.rs` wires `CryptoProvider`, `NetworkClient`, and `TelemetryHandle` together, then prints delivered vs. buffered batches alongside metric snapshots—perfect for a live CLI moment.
- **QS-DAG control plane sim.** `pqcnet-networking/examples/control_plane.rs` boots three logical nodes, exchanges `StateDiff`s, and proves all heads converge. This is the best visual proof that QSTP state sync works end-to-end.
- **Watcher quorum loop.** `pqcnet-sentry/examples/quorum_demo.rs` exercises the same telemetry handle as the relayer, so you can point out that ops has one counter/latency surface across binaries.
- **Telemetry library polish.** `pqcnet-telemetry/src/lib.rs` now enforces feature exclusivity, default flush cadences, and counter overflow detection—talking points for “we already built the hooks for observability.”
- **WASM packaging storyline.** The `autheo-mldsa-dilithium/wasm-demo/README.md` scoping doc explains exactly what ships in the browser module (Kyber + Dilithium only), which is safe to mention when describing front-end readiness.

## Demo Checklist

| Stage | Command | Talking point |
| --- | --- | --- |
| PQC handshake | `cargo run -p autheo-pqc-core --example handshake_demo` | Shows PQC1 record layout, AES-GCM payload, tuple key ID without exposing prod keys. |
| Threshold rotation | `cargo run -p autheo-pqc-core --example secret_sharing_demo` | Visualizes 2-of-3 and 3-of-5 flows, underscoring automated rotation. |
| Relayer queue | `cargo run -p pqcnet-relayer --example pipeline` | Demonstrates batching knobs (`RelayerMode::Bidirectional`, batch size) plus telemetry flush. |
| Control plane | `cargo run -p pqcnet-networking --example control_plane` | Prints discovery + state-sync logs to prove QS-DAG convergence. |
| Watcher quorum | `cargo run -p pqcnet-sentry --example quorum_demo` | Highlights quorum threshold + counters and ties back to telemetry. |
| Web touchpoint | `python3 -m http.server 8000` inside `autheo-mldsa-dilithium/wasm-demo` then open `dilithium.html` | Browser form factors are real but limited to ML-KEM/ML-DSA scope. |

_Tip:_ Run everything from a clean `cargo` workspace with `RUST_LOG=info` to keep logs terse. When showing the WASM demo, mention that the module intentionally excludes legacy crypto per the README.

## Talk Track Outline

- **00:00 – Framing:** “Our PQCNode bundle is now a demoable subsystem that zer0veil can wrap.”
- **02:00 – Crypto proof:** Run the handshake demo; call out PQC1 header fields, tuple key, and AES payload.
- **04:00 – Resilience proof:** Switch to secret sharing demo for the rotation story.
- **06:00 – Throughput proof:** Relayer pipeline output + telemetry counters.
- **08:00 – Network proof:** Control-plane convergence + watcher quorum loop.
- **10:00 – UX glimpse:** WASM page that proves the same primitives load in a browser enclave.
- **11:30 – Close:** Invite deep-dives and point everyone to this doc + `docs/pqcnet-architecture-integration.md`.

## Back-Pocket Answers

- **“Is this production data?”** No. Every demo uses `Config::sample()` or generated keys; nothing maps to real validators.
- **“Can customers run it today?”** Crypto + networking crates already run as Rust binaries; packaging/ops hardening is underway.
- **“What’s next?”** GA heuristics + policy editors stay private, but we can share perf numbers once telemetry exports land.
- **“How is IP protected?”** We only show binaries built from OSS-friendly crates, mock IDs, and wasm bundles scoped to ML-KEM/ML-DSA.

## Next Steps (Post-Demo)

- Dry run the script once with another engineer to time-box the flow.
- Capture screenshots/log snippets from each command so slides can fall back to static images if needed.
- Keep this doc updated—adjust the guardrails if new features graduate to the safe-to-share list.

---

Questions or updates? Ping the PQC platform channel; this doc lives in `docs/` so everyone can iterate safely.
