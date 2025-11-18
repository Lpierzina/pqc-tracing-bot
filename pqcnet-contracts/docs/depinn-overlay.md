# PQCNet DePIN Overlay Architecture

PQCNet already ships PQC handshakes (`autheo-pqc-core` + `autheo-pqc-wasm`), QSTP tunnels, QS-DAG anchoring, and liboqs-backed cryptography. This document layers the missing overlay-specific behaviors, operator workflows, and deployment artifacts so PQCNet can operate as a horizontally scalable DePIN network with incentivized sentry and relayer roles.

---

## 1. Goals & Acceptance Alignment

- **Horizontal scalability** – ≥1,000 nodes coordinate via QS-DAG and Waku pub/sub, with relayer shards that can be added independently.
- **Post-quantum confidentiality & integrity** – All control/data plane messages ride over QSTP tunnels (Kyber ML-KEM + Dilithium ML-DSA) with AES-256-GCM payloads.
- **Operator incentives** – Relayers meter traffic volume per customer/channel; governance contracts consume this data for THEO rewards and potential slashing.
- **Operational hardening** – Remote attestation, key custody (HSM or Shamir shares), Docker/Kubernetes deliverables, and a complete runbook.

---

## 2. Node Roles & Behaviors

### 2.1 Sentry Nodes (Protected RPC Edge)

Responsibility | Implementation Detail
---|---
Client authentication | Extend `autheo-pqc-core::signatures::SignatureManager` to verify ML-DSA (Dilithium) session assertions attached to each client RPC. Reject invalid sessions before any downstream call.
Rate limiting | Embed a dual-layer limiter (per-client token bucket + global concurrency gate) in the forthcoming `pqcnet-sentry` crate. Configuration exposed via Helm `values.yaml` and hot-reloadable via QS-DAG control messages.
Privacy policy enforcement | Interpose an `IdentitySanitizer` middleware that strips, hashes, or replaces identity metadata per tenant policy before forwarding. Policies are hashed and anchored in QS-DAG for auditability.
Attestation logging | Each sentry emits `AttestationEvent` records (Dilithium-signed measurement + enclave hash) via QS-DAG and the Layer-0 anchoring adapter. Events include monotonic counters for slashing evidence.
RPC proxying | QSTP tunnels terminate at the sentry; proxied RPCs onward to validators or application-specific backends reuse ephemeral AES keys derived from the tunnel.

### 2.2 Relayer Nodes (Sharded Transport & Accounting)

Responsibility | Implementation Detail
---|---
Ingress & sharding | Relayers expose a QSTP-ingress endpoint; decrypted frames are enqueued into Kafka or Redis Streams shards selected via `(channel_id || customer_id)` hashing. Each shard owns ordering guarantees.
Ordering & dedup | The new `RelayerShardWorker` module tracks per-topic sequence numbers (leveraging `QstpFrame.seq`). Duplicate suppression uses Redis set-based bloom filters.
Volume metrics | Workers emit `VolumeRecord` structs (customer, channel, bytes, frames, QoS hints) to a Prometheus counter + QS-DAG log. THEO reward calculators pull aggregated stats via gRPC.
Backpressure & persistence | Kafka partitions (or Redis consumer groups) absorb bursts. Flow control signals propagate back through QSTP tunnels (Suspend/Resume frames) to enforce <5% packet loss during churn tests.
Operator incentives | Volume proofs and attestation receipts are combined into `RelayerPerformanceReport` objects signed via Dilithium and published for governance staking/slashing.

---

## 3. Mesh Formation & State Synchronization

1. **Bootstrap** – Nodes receive an initial Waku topic map + QS-DAG peer list via config or onboarding API.
2. **Control Plane (Waku Pub/Sub)** – `autheo-pqc-core::qstp::MeshTransport` gains a Waku adapter that exchanges:
   - Node announcements (role, capabilities, software hash, staking address)
   - Route advertisements / QACE telemetry updates
   - Governance directives (rate limit updates, forced rekeys, slashing notices)
3. **State Sync via QS-DAG** – `qs_dag.rs` extended with:
   - Ledger of relays (attestation hashes, stake, performance history)
   - Attestation record store with append-only edges
   - Meta-routing tables (per-topic shard owners, QoS hints)
4. **Linear Scaling** – Each new relayer shard only needs Kafka/Redis partition metadata + QS-DAG anchor; sentries maintain lightweight pointer caches ensuring O(log n) routing updates. Simulations (`qstp_mesh_sim`) are updated to spawn ≥1,000 nodes using the Waku adapter to validate scaling and recovery.

---

## 4. Security, Attestation & Slashing

- **Remote Attestation** – Nodes leverage Dilithium signing over:
  - Binary hash (WASM or native)
  - Runtime measurements (enclave PCRs, container digests, configuration)
  - Optional TPM/HSM quotes packaged as opaque blobs
- **Publication Flow**
  1. Node signs attestation → pushes to QS-DAG via `QsDagPqc::verify_and_anchor`.
  2. Event mirrored to Layer-0 via IBC-compatible envelope (`AttestationEvent` proto).
  3. Governance indexer ingests events, cross-validates monotonic counters, and flags discrepancies.
- **Slashing** – Governance contracts consume QS-DAG anchors + Layer-0 events to mark nodes faulty. Sentries/relayers subscribe to slashing directives over Waku and auto-disable keys or force re-attestation.

---

## 5. Key Management & Custody

- **HSM-backed nodes** – Production deployments integrate `pqcnet-hsm` adapters for PKCS#11 / Nitro Enclaves. `autheo-pqc-core::key_manager` obtains ML-KEM keys via HSM RPCs; private material never leaves secure hardware.
- **Threshold-protected nodes** – Reuse `secret_sharing.rs` helpers:
  - Keys split into Shamir shares stored across operator vaults.
  - Sentry/relayer processes request shares via gRPC, combine in-memory for single operations, then zeroize.
- **Rekey workflows** – Runbook details forced rekey triggered by governance; QS-DAG stores key version history, and QSTP tunnels automatically renegotiate when key versions change.

---

## 6. Data Plane, Throughput & Resilience

- **QSTP Everywhere** – All node-to-node communication stays on QSTP tunnels (`qstp.rs`). Control-plane Waku messages are sealed payloads, ensuring post-quantum transport even for gossip.
- **Sharded Message Bus** – Kafka (preferred) or Redis Streams:
  - Configure ≥32 partitions per relayer cluster to reach 10,000 TPS aggregate.
  - Use idempotent producers and exactly-once consumers for ordering guarantees.
- **Backpressure** – Relayers monitor partition lag; when exceeding thresholds, they:
  - Send QSTP `FlowControl::Pause` messages upstream.
  - Persist overflow batches to local RocksDB-backed queues for replay.
- **Resilience Testing** – `qstp_performance` example extended to:
  - Spawn synthetic relayer pools, apply chaos (node churn, latency spikes).
  - Verify <5% packet loss by measuring replay success once nodes recover.

---

## 7. Integration & Anchoring

- **Layer-0 / IBC Events** – Introduce `pqcnet-layer0` module:
  - Emits `IBCEnvelope { channel, sequence, payload_hash, signature }`.
  - Used by both sentries (RPC audit logs) and relayers (volume proofs).
- **THEO Accounting** – Volume metrics exported via gRPC/JSON from relayers; governance or staking nodes consume them for reward calculation.
- **QSTP Tunnels** – Remain the default data-plane interface; `autheo-pqc-core/examples/qstp_mesh_sim` updated to showcase sentry→relayer→client routing.

---

## 8. Deployment Artifacts & Runbook

Artifact | Details
---|---
Docker images | Two images (`pqcnet-sentry`, `pqcnet-relayer`). Multi-stage builds compile Rust binaries, inject liboqs artifacts, and harden with distroless bases. Attestation sidecar baked in.
Kubernetes manifests | Helm chart with subcharts per role. Values for: rate limits, Kafka/Redis endpoints, QS-DAG peers, Waku topics, HSM gRPC endpoints, secrets management (Vault, SOPS).
Secrets management | Guidance for Vault + Transit, AWS KMS, or SOPS-managed Shamir shares. Includes init containers that fetch shares/keys before node bootstrap.
Observability | Prometheus metrics (rate limits, queue depth, attestation status, QACE decisions), Grafana dashboards, Loki log aggregation hooks.
Operator runbook | Covers onboarding (stake, attestation), upgrades (rolling with health gates), incident response (force re-attestation, isolate shard, revoke node), and manual failover drills.

---

## 9. Implementation Roadmap

Phase | Workstreams | Notes
---|---|---
1. Overlay scaffolding | New crates `pqcnet-sentry`, `pqcnet-relayer`, Waku transport adapter for `MeshTransport`. Wire existing `autheo-pqc-core` modules into node runtimes. | Heavy reuse of `runtime.rs`, `qstp.rs`, `qs_dag.rs`.
2. Message bus & accounting | Kafka/Redis connectors, shard workers, volume metrics pipeline, Prometheus exporters, governance proto for volume proofs. | Add integration tests + `cargo test -p autheo-pqc-core relayer::*`.
3. Security hardening | Attestation collector, QS-DAG ledger extensions, slashing hooks, HSM adapters, Shamir share services. | Update `docs/qstp.md` + new `docs/attestation.md`.
4. Throughput & resilience | Synthetic harness to hit 10,000 TPS, chaos tests for <5% packet loss, backpressure controls. | Extend `qstp_performance` and add `relayer_burst.rs` example.
5. Deployment & ops | Dockerfiles, Helm charts, CI workflows (build/push images), operator runbook, metrics dashboards. | Validate in distributed testbed before GA.

---

## 10. Acceptance Criteria Traceability

Criterion | Coverage
---|---
Sentry validation & rate limits | Section 2.1 (SignatureManager integration, dual-layer limiter).
Metadata privacy | Section 2.1 (`IdentitySanitizer` + QS-DAG policy anchors).
Attestation logging | Sections 2.1 & 4 (attestation events, QS-DAG anchors).
Relayer sharded queues & metrics | Section 2.2 (Kafka/Redis shards, VolumeRecord pipeline).
Ordering/dedup/backpressure | Sections 2.2 & 6 (sequence tracking, FlowControl, persistence).
Mesh formation & scaling | Section 3 (Waku control plane, QS-DAG sync, ≥1,000 node sim).
Attestation & slashing | Section 4 (Dilithium-signed attestations, governance flow).
Key management & threshold protection | Section 5 (HSM adapters, Shamir shares, forced rekey).
Throughput & resilience targets | Section 6 (10,000 TPS plan, <5% loss chaos tests).
Layer-0 anchoring & QSTP tunnels | Sections 7 & 6 (IBC envelopes, QSTP-first transport).
Deployment artifacts & runbook | Section 8 (Docker, Helm, secrets, observability, incident response).

---

## Next Steps

1. Spin up a new workspace crate (or binary) for `pqcnet-sentry` and `pqcnet-relayer`, reusing `autheo-pqc-core` traits.
2. Implement Waku adapters + message bus sharding logic.
3. Extend QS-DAG traits and protobufs for attestation, volume, and governance directives.
4. Capture operator workflows in the runbook and back them with CI-built Docker images and Helm manifests.
