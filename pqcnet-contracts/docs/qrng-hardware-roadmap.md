# QRNG Hardware & Chronosync Scaling Roadmap

Ken’s Raspberry Pi QRNG bring-up lands in two stages: validating physical entropy sources against the existing CHSH sandbox, then driving Chronosync to 1,000 shards with 50% synthetic noise and QACE reroutes. This note captures the prep work required across PQCNet crates so the hardware feed slots in without more contract changes.

## Stage 1 – Hardware QRNG feed

- **Entropy feed contract (`qrng_feed`)** – `autheo-pqcnet-qrng` now exposes `QrngFeed` so hosts can load `target/chsh_bridge_state.json` or swap in a USB/serial Pi feed. The struct retains the tuple id, shard id, epoch, and 64-char seed hex so every key rotation references an attested QRNG tuple.
- **Harness integration** – `wazero-harness` accepts `--qrng-bridge`, `--qrng-results`, and `--qrng-source` flags. When Ken attaches the Pi, point `--qrng-bridge` at the exported JSON coming off the hardware daemon and the harness will seed the enclave with those bytes, log the CHSH violations, and stamp the handshake envelope with the same metadata.
- **Telemetry** – `pqcnet-telemetry::abw34` defines an ABW34 JSONL schema that captures QRNG provenance (source, tuple id, epoch, seed), shard count, synthetic noise ratio, QACE reroutes, and observed TPS. Both the Go harness and the new Chronosync scaling probe can append to `target/abw34_log.jsonl` so lab runs stay auditable.

## Stage 2 – Chronosync 10 → 100 → 1,000 shards

- **Shard profile config** – `configs/chronosync-shards.toml` declares the baseline (10), expansion (100), and full (1,000) shard topologies with their target global TPS, expected noise ratios, and QACE reroute counts.
- **Scaling probe** – `cargo run -p autheo-pqcnet-chronosync --example scaling_probe` ingests the TOML, computes per-shard throughput, and optionally emits ABW34 entries plus a JSON report for docs/papers. It is parameterized so we can flip to hardware QRNG seeds or custom shard counts without editing the crate.
- **Noise + QACE instrumentation** – the harness exposes `--noise-ratio`, `--shards`, `--tps-per-shard`, and `--qace-reroutes` so we can reproduce the “50% noise with QACE reroutes” scenario while the Chronosync probe reports the same metrics into ABW34.

## Windows harness invocation + end-to-end test recipe

Ken hit a PowerShell quirk when trying to pass flags after `go run`. PowerShell treated the trailing tokens as separate commands because they were entered on new lines without a continuation character. On Windows, keep every flag on one line or use backticks (`` ` ``) for line continuation:

```
go run . `
  -wasm ../pqcnet-contracts/target/wasm32-unknown-unknown/release/autheo_pqc_wasm.wasm `
  -entropy ../pqcnet-contracts/target/wasm32-unknown-unknown/release/autheo_entropy_wasm.wasm `
  -qrng-bridge <bridge_json_from_pi> -qrng-results <results_json_from_pi> `
  -qrng-source hardware:rpi-alpha `
  -abw34-log ../pqcnet-contracts/target/abw34_log.jsonl `
  -shards 1000 -noise-ratio 0.5 -qace-reroutes 120 -tps-per-shard 1500000
```

With that invocation the harness seeds WAMR/WAVEN with the Pi feed (`QrngFeed`), signs the PQC1 envelope, and logs the run to ABW34 without any extra code changes.

### How to test the full AWRE + Chronosync path

1. **Build AWRE artifacts**
   ```
   cd pqcnet-contracts
   rustup target add wasm32-unknown-unknown
   cargo build --release -p autheo-pqc-wasm --target wasm32-unknown-unknown
   cargo build --release -p autheo-entropy-wasm --target wasm32-unknown-unknown
   ```
2. **Produce CHSH evidence** – run `quantum/chsh_sandbox.py` (or the Pi daemon) to emit `chsh_bridge_state.json` + `chsh_results.json`. `QrngFeed` expects the same schema (tuple ids, epochs, 4 KiB hyper-tuples, CHSH stats).
3. **Run the wazero harness** – use the `go run` command above, pointing `-qrng-bridge` / `-qrng-results` at the sandbox files or the Pi capture folder. This exercises WAMR + WAVEN + ABW34 end to end; set `-qrng-source hardware:rpi-alpha` once the Pi feed is live.
4. **Chronosync scaling probe** – `cargo run -p autheo-pqcnet-chronosync --example scaling_probe -- --config pqcnet-contracts/configs/chronosync-shards.toml --abw34-log pqcnet-contracts/target/abw34_log.jsonl --report-json pqcnet-contracts/target/chronosync_profiles.json`. Remember the `--` separator so Clap owns the probe flags.
5. **Regression tests** – `cargo test -p autheo-pqcnet-chronosync`, `cargo run -p autheo-pqcnet-qrng --example qrng_demo`, `cargo test -p pqcnet-telemetry`, and `go test ./wazero-harness/...` keep WAVEN + QRNG plumbing green.

Once the Pi daemon emits the bridge/results JSON, swap the paths and the `qrng_source = "hardware:rpi-alpha"` entries in `configs/chronosync-shards.toml`; ABW34 logs will then capture hardware provenance for the paper’s throughput numbers.

## Publish-ready checklist

1. **Hardware CHSH violations** – run `quantum/chsh_sandbox.py` against the Pi feed, confirm `p < 10^-154`, and log the tuple id + epoch via the ABW34 logger.
2. **Chronosync 1,000 shards** – drive the scaling probe with the `icosuple-1000` profile, record ≥1.5M TPS/shard (≈1.5B TPS aggregate) under a 50% noise ratio and forced QACE reroutes, then snapshot the ABW34 log for the paper.
3. **Documentation** – merge the ABW34 JSONL sample, `qrng-hardware-roadmap.md`, and the Chronosync report output into the final manuscript so reviewers can recreate both the QRNG evidence and the throughput measurements.
