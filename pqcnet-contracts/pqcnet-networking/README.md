# pqcnet-networking

Production-ready networking primitives for PQCNet binaries. `NetworkClient`
pushes real bytes over TCP sockets so validator gateways and external dApps can
reach any PQCNet node exactly the way they will in mainnet. Deterministic,
in-memory transports still exist for unit tests, but the default build never
simulates latency or packet delivery.

## Examples

```
cargo run -p pqcnet-networking --example in_memory_bus
```

The example bootstraps two local peers, sends a unicast message, broadcasts a
payload to every peer, and prints the drained inflight queue so you can trace
what would land on a relayer.

```
cargo run -p pqcnet-networking --example control_plane
```

This spins up three logical nodes that use the Waku-style pub/sub router plus the
QS-DAG module to exchange discovery announcements and converge on identical
state snapshots.

## Code flow diagram

```
dApp / other chain
        |
        v
Relay gateway -----> NetworkClient.publish()/broadcast()
        |                                |
        |                               TCP sockets
        v                                v
   PQCNet peers <---- ControlPlane / PubSub ----> QS-DAG state sync
```

## Control plane building blocks

- `control_plane`: node announcements, control commands, and polling APIs that
  mirror Waku topic/content-topic semantics.
- `pubsub`: programmable router that fan-outs discovery/control envelopes to
  subscribers while preserving per-subscriber buffers.
- `qs_dag` (re-exported from `pqcnet-qs-dag`): append-only DAG with Lamport
  clocks and scoring so nodes can rebuild state from diffs.

The crate re-exports each module so it can be promoted to a standalone,
configurable control-plane repo without API churn.

## Config schema

```toml
[networking]
listen = "0.0.0.0:7300"
max-inflight = 64
jitter-ms = 50

[[networking.peers]]
id = "peer-a"
address = "192.0.2.10:7001"

[[networking.peers]]
id = "peer-b"
address = "198.51.100.23:7002"
```

- `listen` is surfaced to diagnostics so operators know which port a node binds.
- `max-inflight` bounds the internal queue; exceeding it raises an error and
  prevents uncontrolled memory growth.
- `peers` is the static peer set the node can talk to. Each `address` must be a
  routable TCP endpoint—if the socket connection fails the API returns an error.
- `jitter-ms` is only honored when `dev`/`test` features are enabled; production
  builds measure real TCP latency instead of simulating it.

## Tests

```
cargo test -p pqcnet-networking
```

The suite exercises publish/broadcast paths, inflight limit errors, and the
control-plane fan-out logic against actual TCP listeners to guarantee there is
no gap between simulation and production.
