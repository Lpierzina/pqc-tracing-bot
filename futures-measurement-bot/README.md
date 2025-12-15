## Futures Measurement Bot (PQCNet)

A **futures trading measurement bot**: it measures execution quality (fill probability, slippage, rejection rate, latency breakdowns, microstructure response) by consuming an event stream:

- **Strategy intent** (decision time + reference price)
- **Execution events** (send/ack/reject/cancel/fill)
- **Market data snapshots** (top-of-book)

It’s designed to run standalone as a production-shaped Rust crate, while reusing PQCNet components for **auditability and transport primitives**.

### How it works

- **Strategy engine** (your code): emits `StrategyIntent` with:
  - `t_decision`
  - `reference_price` (mid/bbo/mark)
  - optional book snapshot
- **Execution adapter**: emits `OrderSent`, `OrderAck`, `OrderFill`, `OrderCancelled`, `OrderRejected` with raw timestamps.
- **Metrics engine**: correlates `intent_id → order_id → fills` and updates per-bucket stats.
- **Audit sinks (PQCNet-backed)**:
  - **TupleChain** (`autheo-pqcnet-tuplechain`): persists each audit event as a tuple with commitments/sharding.
  - **QS-DAG** (`pqcnet-qs-dag`): anchors each audit event as a DAG diff (in-memory in this repo) for append-only audit graph semantics.

### Metrics computed

- **Fill probability**: per bucket, terminal outcomes
  - \(\text{fills} / (\text{fills} + \text{cancels} + \text{timeouts})\)
- **Slippage** (bps): compares final VWAP fill to decision reference price
  - tracked separately as **adverse** vs **favorable**
- **Rejection rate**: \(\text{rejected} / \text{submitted}\)
- **Latency** (ms histograms):
  - decision→send
  - send→ack
  - send→first fill
  - send→last fill
- **Microstructure response** (bps histograms): mid drift after fill at horizons (100ms/1s/5s by default)

Buckets include: symbol, venue, side, order type, quantity bucket, and a coarse time-of-day bucket.

### Why PQCNet helps here

- **TupleChain gives an audit ledger primitive**
  - Each measurement event becomes a tuple with a deterministic commitment, versioning, and sharding.
  - You get an evidence trail suitable for post-trade forensics and compliance workflows.

- **QS-DAG gives an append-only audit graph model**
  - The measurement stream can be represented as DAG diffs with attached tuple envelopes.
  - This is a natural fit for "what happened when" audit timelines and anchoring.

- **QSTP integration surface exists**
  - PQCNet’s QSTP crate includes tuple-pointer patterns and routing metadata.
  - In production, the same measurement events can be moved over QSTP tunnels and anchored identically.

In short: PQCNet turns "metrics logs" into **evidence artifacts**.

### Run the demo bot

From this repo directory:

```bash
cargo run --bin bot -- --venue autheo --symbol ES --side buy --order-type limit --qty 1 --iters 50
```

It prints a small KV snapshot (e.g., fill probability) at the end.

### Test

```bash
cargo test
```

This runs unit tests that validate fill probability, slippage directionality, and basic correlation logic.
