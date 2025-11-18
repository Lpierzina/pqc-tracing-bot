## PQCNet Binary Reference

This document summarizes the two new binaries (`pqcnet-sentry`, `pqcnet-relayer`), the feature flags they honor, supported CLI arguments, and the shared configuration schemas (TOML and YAML). Sample files live in `configs/`.

### Feature Flags
- `dev`: favors fast iteration (short TTLs, aggressive telemetry flushing).
- `test`: mirrors CI behavior with moderate timeouts/retries.
- `prod`: default; longest TTLs and conservative retry budgets.

Each binary and shared library exposes the three mutually exclusive cargo features, allowing `cargo run -p pqcnet-sentry --features dev` for local smoke tests.

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

[crypto]
node-id = "relayer-a"
secret-seed = "bbbbbbbb..."
```

`configs/pqcnet-relayer.yaml`
```
relayer:
  mode: bidirectional
  batch-size: 4
crypto:
  node-id: relayer-a
  secret-seed: bbbbbbbb...
```

Refer to the config modules (`pqcnet-sentry/src/config.rs`, `pqcnet-relayer/src/config.rs`) for full serde-driven schemas and validation rules.
