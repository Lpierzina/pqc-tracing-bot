## PQCNet Binary Reference

This document summarizes the two new binaries (`pqcnet-sentry`, `pqcnet-relayer`), the feature flags they honor, supported CLI arguments, and the shared configuration schemas (TOML and YAML). Sample files live in `configs/`.

### Feature Flags
- `dev`: favors fast iteration (short TTLs, aggressive telemetry flushing).
- `test`: mirrors CI behavior with moderate timeouts/retries.
- `prod`: default; longest TTLs and conservative retry budgets.

Each binary and shared library exposes the three mutually exclusive cargo features, allowing `cargo run -p pqcnet-sentry --features dev` for local smoke tests.

### AWRE/WAVEN runtime profile

- Both binaries load the Autheo WASM Runtime Engine (AWRE) profile via `AWRE_PROFILE=awre-waven` (or `--awre-profile awre-waven` if you pass it explicitly). That profile pins the wasm-micro-runtime commit, interpreter/AOT/JIT tiers, and WAVEN MMU toggles so DePIN overlays inherit the same enclave posture as production PQCNet nodes.
- The runtime also expects `qrng_feed` metadata (`QRNG_FEED_PATH`, optional `--qrng-feed`) before any PQCNet overlay spins up. The ABW34 tuple id exported from the feed becomes part of the sentry/relayer telemetry labels so DAO voters can cross-check entropy provenance.
- `scripts/awre_waven_verify.sh` reads the profile + wavm measurements, confirms WAVEN dual page tables remain enabled, and emits the hash that CI, GitOps, and runbooks cite. Keep that script in every deployment artifact (Kubernetes init containers, bare-metal bootstrap).

### CLI Arguments

`pqcnet-sentry`
- `--config <path>`: default `configs/pqcnet-sentry.toml`.
- `--config-format <auto|toml|yaml>`: overrides file detection.
- `--iterations <n>`: number of event loop iterations before exit (default 1).
- `--dry-run`: skips networking while still emitting telemetry events.

`pqcnet-relayer`
- `--config <path>`: default `configs/pqcnet-relayer.toml`.
- `--config-format <auto|toml|yaml>`.
- `--mode <ingest|egress|bidirectional>`: optional override of the config value.
- `--batch-size <n>`: optional override of the relayer batch size.
- `--iterations <n>`: number of processing loops to run (default 1).

### Config Schema Overview

Both binaries share the same top-level sections:
- `crypto`: `node-id`, `secret-seed` (64 hex chars), `key-ttl-secs`.
- `networking`: `listen`, optional `peers[] { id, address }`, `max-inflight`, `retry-attempts`, `jitter-ms`.
- `telemetry`: `endpoint`, `flush-interval-ms`, optional `labels` map.

Binary-specific sections:
- `sentry`: `watchers[]`, `poll-interval-ms`, `quorum-threshold`.
- `relayer`: `mode`, `batch-size`, `max-queue-depth`, `retry-backoff-ms`.

### Sample Configs

`configs/pqcnet-sentry.toml`
```
[sentry]
watchers = ["relayer-a", "relayer-b"]
poll-interval-ms = 1500
quorum-threshold = 2

[runtime]
awre-profile = "awre-waven"
qrng-feed-path = "/var/pqcnet/qrng_feed"
abw34-export = "/var/pqcnet/abw34"

[crypto]
node-id = "sentry-a"
secret-seed = "aaaaaaaa..."

[networking]
listen = "0.0.0.0:7100"

[telemetry]
endpoint = "http://localhost:4318"
```

`configs/pqcnet-sentry.yaml`
```
sentry:
  watchers: ["relayer-a", "relayer-b"]
runtime:
  awre-profile: awre-waven
  qrng-feed-path: /var/pqcnet/qrng_feed
  abw34-export: /var/pqcnet/abw34
crypto:
  node-id: sentry-a
  secret-seed: aaaaaaaa...
```

`configs/pqcnet-relayer.toml`
```
[relayer]
mode = "bidirectional"
batch-size = 4
max-queue-depth = 512
retry-backoff-ms = 250

[crypto]
node-id = "relayer-a"
secret-seed = "bbbbbbbb..."

[runtime]
awre-profile = "awre-waven"
waven-dual-pt = true
qrng-feed-path = "/var/pqcnet/qrng_feed"
```

`configs/pqcnet-relayer.yaml`
```
relayer:
  mode: bidirectional
  batch-size: 4
  max-queue-depth: 512
  retry-backoff-ms: 250
runtime:
  awre-profile: awre-waven
  waven-dual-pt: true
  qrng-feed-path: /var/pqcnet/qrng_feed
crypto:
  node-id: relayer-a
  secret-seed: bbbbbbbb...
```

> The `[runtime]` / `runtime:` sections above are metadata consumed by the AWRE/WAVEN bootstrap + verification scripts (`scripts/awre_waven_verify.sh`, Helm init containers). The Rust binaries ignore unknown sections, so it is safe to keep these hints alongside the canonical crypto/networking settings.

Refer to the config modules (`pqcnet-sentry/src/config.rs`, `pqcnet-relayer/src/config.rs`) for full serde-driven schemas and validation rules.
