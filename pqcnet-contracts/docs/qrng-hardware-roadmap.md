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

## Publish-ready checklist

1. **Hardware CHSH violations** – run `quantum/chsh_sandbox.py` against the Pi feed, confirm `p < 10^-154`, and log the tuple id + epoch via the ABW34 logger.
2. **Chronosync 1,000 shards** – drive the scaling probe with the `icosuple-1000` profile, record ≥1.5M TPS/shard (≈1.5B TPS aggregate) under a 50% noise ratio and forced QACE reroutes, then snapshot the ABW34 log for the paper.
3. **Documentation** – merge the ABW34 JSONL sample, `qrng-hardware-roadmap.md`, and the Chronosync report output into the final manuscript so reviewers can recreate both the QRNG evidence and the throughput measurements.
