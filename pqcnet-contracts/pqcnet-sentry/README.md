# pqcnet-sentry

Reference sentry daemon that polls configured relayers/watchers, derives
per-watcher challenges, and ships synthetic telemetry. The crate now exposes a
library interface so doctests and examples can exercise the service logic
without going through the CLI binary.

## Examples / Demos

Watch a single iteration with the new example:

```
cargo run -p pqcnet-sentry --example quorum_demo
```

Or drive the real CLI with a config file:

```
cargo run -p pqcnet-sentry -- --config configs/pqcnet-sentry.toml --iterations 3
```

Both flows print processed watchers, quorum thresholds, and telemetry counters so
your PM can see the sentry making progress.

## Config schema

```toml
[sentry]
watchers = ["peer-a", "peer-b"]
poll-interval-ms = 2000
quorum-threshold = 2

# crypto/networking/telemetry sections omitted for brevity; see the other crate READMEs
```

- `watchers` is the peer list the sentry challenges every iteration.
- `poll-interval-ms` controls how frequently the CLI loops (examples override the
  loop length by running a single iteration).
- `quorum-threshold` determines how many successful watcher responses are
  required before the sentry reports success.

See `configs/pqcnet-sentry.toml` (and `.yaml`) for a complete config including
the shared `[crypto]`, `[networking]`, and `[telemetry]` sections.

## Tests

```
cargo test -p pqcnet-sentry
```

The suite covers config validation, TOML/YAML decoding, service telemetry, and
now the doctest embedded in `service.rs`, ensuring `cargo test --doc` reports
non-zero coverage.
