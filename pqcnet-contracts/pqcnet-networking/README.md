# pqcnet-networking

In-memory message bus for pqcnet demos. It keeps networking deterministic and
observable so service-level tests (relayer, sentry, telemetry probes) can run
without sockets.

## Examples

```
cargo run -p pqcnet-networking --example in_memory_bus
```

You will see a unicast send, a broadcast fan-out, and the inflight queue draining
into readable logs that mimic what the relayer emits.

```
cargo run -p pqcnet-networking --example control_plane
```

This spins up three logical nodes that use the Waku-style pub/sub router plus the
QS-DAG module to exchange discovery announcements and converge on identical
state snapshots.

## Control plane building blocks

- `control_plane`: node announcements, control commands, and polling APIs that
  mimic Waku topic/content-topic semantics.
- `pubsub`: deterministic in-memory pub/sub router mirroring `pubsubTopic` and
  `contentTopic` layers for discovery/control traffic.
- `qs_dag`: append-only DAG with Lamport clocks and a light scoring rule so
  nodes can rebuild state from diffs.

The crate re-exports each module so it can be promoted to a standalone
“configurable control plane” repository without reshuffling APIs.

## Config schema

```toml
[networking]
listen = "0.0.0.0:7300"
max-inflight = 64
jitter-ms = 50

[[networking.peers]]
id = "peer-a"
address = "127.0.0.1:7301"

[[networking.peers]]
id = "peer-b"
address = "127.0.0.1:7302"
```

- `listen` is only for diagnostics but matches what CLI flags expect.
- `max-inflight` bounds the internal queue; exceeding it raises an error.
- `peers` is a static peer set; IDs are used in the crypto-derived payloads.

## Tests

```
cargo test -p pqcnet-networking
```

The test suite covers publish/broadcast paths, inflight limit errors, and now the
quickstart doctest so `cargo test --doc` includes at least one case.
