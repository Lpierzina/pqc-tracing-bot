# pqcnet-relayer

Reference relayer daemon that batches PQC messages, broadcasts them to peers,
and records telemetry suitable for demos/tests. The crate now exposes a library
surface so doctests and examples can drive the queue logic without the CLI.

## Examples / Demos

```
cargo run -p pqcnet-relayer --example pipeline
```

The pipeline example fills the queue, performs a bidirectional batch, and dumps
the telemetry snapshot so you can demonstrate delivery counts and buffered
messages.

Run the CLI with a real config:

```
cargo run -p pqcnet-relayer -- --config configs/pqcnet-relayer.toml --iterations 5
```

Override the relayer mode or batch size on the command line to show hot
reconfiguration.

## Config schema

```toml
[relayer]
batch-size = 8
max-queue-depth = 2048
retry-backoff-ms = 500
mode = "bidirectional" # ingest | egress | bidirectional

# crypto/networking/telemetry sections omitted for brevity; see the other crate READMEs
```

- `batch-size` controls how many messages are drained per iteration.
- `max-queue-depth` caps memory usage and safeguards backpressure.
- `mode` toggles ingest-only, egress-only, or bidirectional loops.

See `configs/pqcnet-relayer.toml` (and `.yaml`) for a complete config.

## Tests

```
cargo test -p pqcnet-relayer
```

Tests cover config validation, service telemetry, queue behavior, and the new
doctest embedded in `service.rs`, so `cargo test --doc` reports active cases.
