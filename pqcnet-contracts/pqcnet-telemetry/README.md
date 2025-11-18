# pqcnet-telemetry

Structured telemetry handle for pqcnet demos. It records counters and latency
histograms in-memory so services can assert on instrumentation without spinning
up OpenTelemetry collectors.

## Example / Demo

```
cargo run -p pqcnet-telemetry --example flush_snapshot
```

The example increments a counter, records latencies, and prints the flushed
snapshot to prove that instrumentation works end-to-end.

## Config schema

```toml
[telemetry]
endpoint = "http://localhost:4318"
flush-interval-ms = 500

[telemetry.labels]
component = "sentry"
cluster = "devnet"
```

- `endpoint` is informational in the mock implementation but mirrors production.
- `flush-interval-ms` drives timers inside higher-level services.
- `labels` are baked into every snapshot and make it easy to filter logs.

## Tests

```
cargo test -p pqcnet-telemetry
```

Unit tests cover counter overflow detection, state clearing, and the doctest
keeps `cargo test --doc` from reporting zero cases.
