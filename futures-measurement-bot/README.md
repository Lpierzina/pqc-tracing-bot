## Futures Measurement Bot (PQCNet)

A **futures trading execution-quality measurement bot**.

This crate is intentionally **not** an “alpha bot”. It’s the plumbing you wrap around any strategy/execution stack to answer:

- Did we get filled when we expected to?
- How much slippage did we take (adverse vs favorable)?
- Where is latency coming from (decision→send, send→ack, send→fill)?
- What happened to the market *after* we traded (microstructure response)?
- Can we turn the measurement stream into **evidence artifacts** (audit trail), not just logs?

It does that by ingesting a single ordered stream of events:

- `StrategyIntent` (what the strategy decided, when, with what reference price)
- `OrderSent` / `OrderAck` / `OrderRejected` / `OrderCancelled` / `OrderFill` (what execution did)
- `MarketData` (top-of-book snapshots)

…and correlating everything into per-bucket stats.

---

### Big picture: how the pieces fit

```
┌──────────────────┐
│ Strategy engine  │
│ (your code)      │
└───────┬──────────┘
        │ emits Event::StrategyIntent
        v
┌──────────────────┐            ┌──────────────────────────┐
│ MetricsEngine    │◄───────────│ Market data (top-of-book)│
│ (correlate+stats)│   observes │ Event::MarketData         │
└───────┬──────────┘            └──────────────────────────┘
        │ observes
        │ Event::Order* (send/ack/reject/cancel/fill)
        v
┌──────────────────┐
│ Execution adapter │
│ (venue-specific)  │
└──────────────────┘

In parallel:
  MetricsEngine -> Telemetry (OTLP-ish) + Audit sinks (TupleChain + QS-DAG)
```

There are two “front doors” in this repo:

- `src/bin/bot.rs`: a CLI demo that simulates market data, emits intents, calls an execution adapter, and feeds all resulting events into the `MetricsEngine`.
- `src/bin/web_ui.rs`: a small server that serves a static browser UI, relays a websocket feed (simulated or Tastytrade streamer), and exposes a rescue-route scanner API.

---

### Core data model

All measurement input is represented by the `Event` enum (`src/events.rs`). A typical lifecycle looks like:

1) `MarketData` (snapshot before decision)
2) `StrategyIntent` (includes `intent_id`, `OrderParams`, optional `reference_price`, optional `book`)
3) `OrderSent` (binds `intent_id` → `order_id`)
4) `OrderAck` (optional, but enables send→ack latency)
5) One terminal outcome:
   - `OrderFill` (may be partials; a final fill is marked by `is_final = true`)
   - `OrderCancelled`
   - `OrderRejected`
   - Or a **timeout** detected by `MetricsEngine::tick()` when no terminal outcome arrives within `fill_timeout_ms`

The only requirement is that you feed the engine a coherent stream. The engine does not talk to venues directly.

---

### Correlation logic (intent → order → fills)

The `MetricsEngine` (`src/metrics/engine.rs`) correlates in two phases:

- **At intent time**: it creates an `OrderState` keyed by a placeholder `OrderId("intent:<intent_id>")`. This lets you start timing from decision immediately.
- **At send time** (`OrderSent`): it removes the placeholder state and re-inserts it under the real `order_id`, binding `intent_id → order_id`.

From that point on, all subsequent `OrderAck` / `OrderFill` / `OrderCancelled` are keyed by `order_id`.

Rejections are handled defensively: a venue might reject before an `order_id` exists, so `OrderRejected` supports `intent_id` and/or `order_id`.

---

### Bucketing (how stats are grouped)

Bucket keys live in `src/buckets.rs`.

Every intent/order is assigned a `BucketKey`:

- `symbol`
- `venue`
- `side`
- `order_type`
- `qty_bucket` (micro/small/medium/large)
- `tod` (coarse UTC time-of-day bucket)

This lets you compute “fill probability for ES, Autheo, Buy, Limit, Small size, US hours” instead of a single global average.

---

### Metrics computed (what “measurement” means here)

Metrics are stored per bucket in `BucketStats` (`src/metrics/stats.rs`) and surfaced via `snapshot_kv()` / `snapshot_buckets()`.

- **Fill probability**
  - `filled / (filled + cancelled + timed_out)`
- **Rejection rate**
  - `rejected / submitted`
- **Slippage (bps histograms)**
  - Compares **final VWAP** to `StrategyIntent.reference_price`.
  - Slippage is split into:
    - **adverse**: worse than reference (buys above reference; sells below reference)
    - **favorable**: better than reference
- **Latency (ms histograms)**
  - decision→send
  - send→ack
  - send→first fill
  - send→last fill
- **Microstructure response (bps histograms)**
  - At configurable horizons (default 100ms / 1s / 5s), it measures mid-price drift using the latest available `MarketData`.
- **Decision-time spread/depth context**
  - Spread bps at decision, and bid/ask depth (sum across top-N)

Important nuance: microstructure horizons are recorded when you call `MetricsEngine::tick(now)` and the engine can find a recent `MarketData` snapshot for the (venue,symbol).

---

### Telemetry

`MetricsEngine` also records counters/latencies via `pqcnet-telemetry`.

- Default endpoint: `http://localhost:4318` (see `MetricsConfig.telemetry_endpoint`)
- If you don’t have an OTLP collector running, the demo still works; telemetry is a best-effort side-channel.

---

### Audit trail (why PQCNet is involved)

This repo treats measurement events as **audit-worthy**: “what happened when” should be representable as immutable-ish evidence, not just text logs.

Audit is abstracted behind `AuditSink` (`src/audit/mod.rs`). The CLI demo (`bot`) fans out to two sinks:

- **TupleChain** (`src/audit/tuplechain.rs` via `autheo-pqcnet-tuplechain`)
  - Each audit event is stored as a tuple (subject/predicate/object)
  - A proof/commitment is attached (demo uses signature scheme wiring)
  - The tuple store can shard and support simple querying

- **QS-DAG** (`src/audit/qsdg.rs` via `pqcnet-qs-dag`)
  - Each audit event is anchored as a `StateDiff` with an attached `TupleEnvelope` (domain=Finance)
  - In this repo it’s an in-memory DAG, but the shape matches an append-only audit graph model

In short: PQCNet turns “metrics logs” into **structured artifacts** that are easier to anchor, route, and verify.

---

### Execution adapters (venue integration surface)

Execution is intentionally separated behind `ExecutionAdapter` (`src/execution/mod.rs`).

This repo includes **stub/simulated** adapters to show the event contract:

- `AutheoAdapter` (`src/execution/autheo.rs`)
  - Demonstrates the intended QSTP/TupleChain integration surface (in-memory in this demo)
  - Emits send/ack and probabilistic fill/cancel events with simulated timings

- `TradingStationAdapter` (`src/execution/trading_station.rs`)
  - Another simulated venue path with different latency/fill profile

In production, your adapter would talk to a real order API / FIX gateway / broker SDK and emit the same event types with raw timestamps.

---

### Run the measurement demo (CLI)

From this crate directory:

```bash
cargo run --bin bot -- --venue autheo --symbol ES --side buy --order-type limit --qty 1 --iters 50
```

It prints a JSON key/value snapshot at the end (including per-bucket fill probability).

---

### Web UI demo (static UI + websocket relay)

This repo includes a static browser UI plus a small Rust server:

- Serves `web-ui/` (HTML/JS/CSS)
- Exposes a local websocket at `/ws` for the browser
- In **sim mode**, emits a fake quote stream (`SIM`) ~4 times/second
- In **Tastytrade streamer mode**, connects to a dxLink-style upstream websocket and forwards messages
- Serves PQC WASM at `/wasm/autheo_pqc_wasm.wasm` so the browser can run a PQCNet handshake demo
- Exposes `POST /api/rescue_scan` for the Distressed Position Rescue Scanner
- Includes a **live Open-MA trend-window detector** in the browser UI (no backend required)

Note: the server also exposes `/api/metrics/*`, but the current web UI server does not yet ingest any measurement `Event`s into its `MetricsEngine` (it’s a placeholder for wiring a live event stream later).

#### Distressed Position Rescue Scanner

A concrete, interactive example of “strategy-shaped analytics” living next to the measurement plumbing:

- You enter a vertical spread (short strike, long strike, DTE, IV, underlying)
- The server computes theoretical prices/Greeks (self-contained Black–Scholes)
- It enumerates candidate rescue routes along:
  - **roll out** (increase DTE)
  - **roll down** (shift strikes)
  - **widen** (increase width)
- It ranks candidates to prefer positive theta and/or improved break-even, with a small penalty for extra risk

This is for exploration and demo UX, not trading advice.

#### 1) Start the server (simulated stream)

```bash
STREAM_SIM=1 cargo run --bin web_ui
```

Open `http://localhost:8080/` and hit **Connect**.

If you’re running from the repo root (no `cd`):

```bash
cargo run --manifest-path futures-measurement-bot/Cargo.toml --bin web_ui
```

#### 2) Start the server (real Tastytrade streamer)

Set credentials as server env vars (don’t put tokens in browser JS):

```bash
export TASTYTRADE_STREAMER_URL="wss://<dxlink-streamer-host>/..."
export TASTYTRADE_STREAMER_TOKEN="<streamer-token>"
cargo run --bin web_ui
```

Open `http://localhost:8080/`, click **Connect**, then use **Subscribe**.

#### Open-MA trend-window detector (live chart + BEGIN/END alerts)

The UI includes a live “open moving averages” detector that marks contiguous **trend windows** and emits the same “begin/end”
concept as the circled points in the prompt.

- **Input**: the live websocket stream (simulated `SIM` or forwarded dxLink messages).
- **Signal definition** (mirrors `src/strategy/open_ma_trend.rs`):
  - Compute SMA(fast) and SMA(slow) on the observed price series (oldest → newest).
  - At bar \(i\), classify as:
    - **UP** if \( \text{SMA}_{fast} > \text{SMA}_{slow} \) and both SMA slopes are positive.
    - **DOWN** if \( \text{SMA}_{fast} < \text{SMA}_{slow} \) and both SMA slopes are negative.
  - Require the averages to be “open”:
    - Gap constraint: \( |\text{SMA}_{fast} - \text{SMA}_{slow}| / |\text{SMA}_{slow}| \ge \text{min\_gap\_pct} \)
  - Require minimum momentum:
    - Slope constraint: for a lookback of `slope_lookback` bars, both SMAs must have
      \( |\Delta \text{SMA} / \text{SMA}| \) per bar ≥ `min_slope_pct_per_bar`.
- **BEGIN/END semantics**:
  - A window **begins** at the first bar where the classifier returns UP/DOWN.
  - A window **ends** at the last bar before the classifier becomes `None` (or switches direction).

**How to use it**

- Start `web_ui` (sim or streamer mode), open `http://localhost:8080/`, and connect your feed.
- In the **Open-MA Trend Window Detector** card:
  - Set **Symbol to watch** (default `SIM` in sim mode).
  - Pick **Price source**:
    - `Last` works for sim mode and many trade feeds.
    - `Mid` uses \( (bid+ask)/2 \) if both are present.
  - Tune `fast/slow`, `slope lookback`, `min gap`, and `min slope` if needed.
- Watch the chart:
  - The shaded region indicates the currently active open-MA phase (green=UP, red=DOWN).
  - Vertical markers show detected BEGIN/END points as they occur.

---

### Build / embed PQCNet WASM (for the browser demo)

The UI requests:

- `GET /wasm/autheo_pqc_wasm.wasm`

By default, the server looks for a built artifact at:

- `../pqcnet-contracts/target/wasm32-unknown-unknown/release/autheo_pqc_wasm.wasm`

Build it from the repo root:

```bash
cd ../pqcnet-contracts
rustup target add wasm32-unknown-unknown
cargo build --release -p autheo-pqc-wasm --target wasm32-unknown-unknown
```

Override the path:

```bash
AUTHEO_PQC_WASM_PATH="/absolute/path/to/autheo_pqc_wasm.wasm" cargo run --bin web_ui
```

Or copy a prebuilt artifact into:

- `web-ui/wasm/autheo_pqc_wasm.wasm`

---

### Extending / integrating

- **Plug in real market data**: feed `Event::MarketData` into `MetricsEngine::observe()`.
- **Plug in a real strategy**: emit `Event::StrategyIntent` with a stable `intent_id` and a decision-time `reference_price`.
- **Plug in a real venue**: implement `ExecutionAdapter` and emit `Event::Order*` with accurate timestamps.
- **Drive time**: call `MetricsEngine::tick(now)` periodically to process timeouts and microstructure horizons.
- **Change horizons/timeouts**: pass a custom `MetricsConfig`.
- **Swap audit backends**: implement `AuditSink` (or use `CompositeAuditSink` to fan out).

---

### Test

```bash
cargo test
```

This validates basic correlation logic and the rescue scanner.
