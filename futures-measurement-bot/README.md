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

### Run the browser demo UI (Streamer + PQCNet WASM)

This repo also includes a **static HTML demo UI** plus a small Rust server that:

- Serves `web-ui/` (HTML/JS/CSS)
- Exposes a **local websocket** at `/ws` for the browser
- Connects to the **Tastytrade Streamer websocket** (or a simulator) and relays messages to the browser
- Serves `autheo_pqc_wasm.wasm` at `/wasm/autheo_pqc_wasm.wasm` so the browser can run a PQCNet handshake

#### Distressed Position Rescue Scanner (feature)

The web UI includes a **Distressed Position Rescue Scanner** that helps you explore “escape routes” for a short-premium vertical spread by comparing:

- **Break-even** (better for put spreads = lower; better for call spreads = higher)
- **Theta/day** (prefers positive time decay)
- **Capital at risk** (allowed to increase, but penalized)

**Why it’s here**

- It’s a concrete, interactive example of “strategy-shaped” analytics living next to the bot + audit/metrics plumbing.
- It’s **self-contained** (no external quant dependencies): a simple Black–Scholes implementation + a deterministic candidate grid so demos are reproducible.
- It gives a fast way to sanity-check “roll out / roll down / widen” adjustments in a consistent scoring framework (not a recommendation engine).

**How it works (high level)**

- You provide a vertical (short strike / long strike / DTE / IV / underlying).
- The engine computes theoretical leg prices + Greeks (delta/theta/vega) and derives spread metrics (credit, break-even, theta/day, capital at risk).
- It enumerates candidate routes across three axes:
  - **Roll out**: increases DTE across a fixed grid
  - **Roll down**: shifts strikes (down for puts, up for calls)
  - **Widen**: moves the long leg further OTM to increase width
- Candidates are filtered/ranked to prefer **positive theta** and/or an **improved break-even**, with a small penalty for extra risk.

> Note: this is a simplified model intended for exploration and UI demos, not trading advice.

#### 1) Start the server (simulated stream)

```bash
STREAM_SIM=1 cargo run --bin web_ui
```

Open `http://localhost:8080/` and hit **Connect**. You should see a `SIM` stream updating ~4 times/second.

If you’re running from the repo root (no `cd`), use:

```bash
cargo run --manifest-path futures-measurement-bot/Cargo.toml --bin web_ui
```

Then:

- Open `http://localhost:8080/`
- Scroll to **Distressed Position Rescue Scanner**
- Enter your spread inputs (e.g. PLTR: **short strike / long strike / DTE / IV / underlying**)
- Click **Scan rescue routes**

#### 2) Start the server (real Tastytrade Streamer)

For the demo server, **do not** put API tokens in browser JS. Instead, set them as server env vars:

```bash
export TASTYTRADE_STREAMER_URL="wss://<dxlink-streamer-host>/..."
export TASTYTRADE_STREAMER_TOKEN="<streamer-token>"
cargo run --bin web_ui
```

Then open `http://localhost:8080/`, click **Connect**, and use **Subscribe** to request symbols/feeds.

Notes:

- The dxLink protocol details can change; the UI includes a **Raw send** box so you can try exact JSON payloads without redeploying.
- The server currently expects `TASTYTRADE_STREAMER_URL` + `TASTYTRADE_STREAMER_TOKEN` (recommended for demos). A login/token-fetch flow can be added later if needed.

#### 3) Build / embed PQCNet WASM

The UI tries to fetch `GET /wasm/autheo_pqc_wasm.wasm`.

By default, the server looks for a built artifact at:

- `../pqcnet-contracts/target/wasm32-unknown-unknown/release/autheo_pqc_wasm.wasm`

Build it from the repo root:

```bash
cd ../pqcnet-contracts
rustup target add wasm32-unknown-unknown
cargo build --release -p autheo-pqc-wasm --target wasm32-unknown-unknown
```

If you want to override where the server loads the WASM from:

```bash
AUTHEO_PQC_WASM_PATH="/absolute/path/to/autheo_pqc_wasm.wasm" cargo run --bin web_ui
```

If you don’t want to build WASM, you can also copy a prebuilt `autheo_pqc_wasm.wasm` into:

- `web-ui/wasm/autheo_pqc_wasm.wasm`

### Test

```bash
cargo test
```

This runs unit tests that validate fill probability, slippage directionality, and basic correlation logic.
