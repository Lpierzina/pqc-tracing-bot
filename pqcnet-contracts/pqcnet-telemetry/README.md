# pqcnet-telemetry

Production-grade instrumentation for PQCNet binaries. Every counter/latency that
`pqcnet-sentry`, `pqcnet-relayer`, or any standalone node records is flushed over
real OTLP/HTTP so downstream collectors (OTel, Honeycomb, Grafana Cloud, etc.)
see the exact payloads that live deployments emit. No simulations, no
short-circuiting.

## How it works

1. `TelemetryHandle::from_config` bootstraps labels + flush cadence shared by a
   PQCNet node (sentry, relayer, validator, dApp gateway).
2. `record_counter` and `record_latency_ms` update thread-safe maps immediately
   when traffic enters from dApps or validator gossip.
3. `record_kem_event` logs a `KemUsageRecord` (label, scheme, rationale,
   backup-only) so Kyber/HQC drills and Dilithium+SPHINCS redundancy policies are
   captured alongside counters.
4. `flush()` snapshots the current state and POSTs JSON to the configured
   collector endpoint. The method returns a `Result` so services can surface
   export failures instead of silently dropping data.
5. Callers usually flush after each control-plane iteration (relayers) or at the
   end of reconciliation loops (sentries) so every request coming from other
   chains is observable.

## Code flow diagram

```mermaid
%%{init: { "theme": "neutral" }}%%
flowchart LR
    dapp["dApp / Relay Gateway"]
    node["PQCNet node\n(relayer, sentry, overlay)"]
    handle["TelemetryHandle\n(counters, latencies, KEM events)"]
    snapshot["Snapshot (counters + KemUsageRecord)"]
    exporter["HTTP exporter"]
    collector["Collector / OTLP sink"]
    dashboard["Dashboard / auditor view"]

    dapp --> node
    node -->|record_counter / record_latency| handle
    node -->|record_kem_event| handle
    node -->|flush()| snapshot
    snapshot --> exporter --> collector --> dashboard
```

## Example

```
cargo run -p pqcnet-telemetry --example flush_snapshot
```

The example spins up a throwaway HTTP sink, records ingest counters, exports the
payload, and prints the snapshot so you can inspect the exact JSON hitting your
collector.

## Config schema

```toml
[telemetry]
endpoint = "http://collector.pqcnet.io:4318"
flush-interval-ms = 500

[telemetry.labels]
component = "sentry"
cluster = "validator-net"
```

- `endpoint` must point at a real OTLP/HTTP collector; the crate will return an
  error if it cannot connect or receives a non-2xx response.
- `flush-interval-ms` controls how aggressively services push instrumentation to
  the backend.
- `labels` are attached to every snapshot so cross-chain traffic (e.g. dApps
  coming from other L2s) stays filterable.

## PQC telemetry (KemUsageRecord)

- `record_kem_event(KemUsageRecord)` accepts a label (e.g., `relayer::key-id`),
  the stringified KEM scheme or `signature-stack`, the reason (`normal`, `drill`,
  `fallback`), and whether the plan was marked `backup_only`.
- Relayers, sentries, and DW3B overlays call this when they advertise ML-KEM keys
  or enforce the Dilithium+SPHINCS redundancy policy, giving auditors a timeline
  of Kyber/HQC posture per node.
- The records flow out with every snapshot so dashboards can alert when nodes
  flip to HQC drills or single-signature fallback modes.

## Tests

```
cargo test -p pqcnet-telemetry
```

Tests stand up ephemeral collectors to ensure real HTTP payloads are emitted,
cover counter overflow detection, and track state clearing semantics.
