# QSTP vs TLS Performance Snapshot

Measured with the lab harness (200 iterations, Ryzen 9 / Linux 6.1) that pairs
`cargo test -p pqcnet-qstp` with the `wazero-harness` host runner using
deterministic Kyber/Dilithium adapters.

| Metric (avg) | QSTP | TLS | Delta |
| --- | --- | --- | --- |
| Handshake latency | 0.280 ms | 0.814 ms | **-65.66%** |
| Payload seal/open | 0.149 ms | 0.013 ms | +1041.41% |
| End-to-end (handshake + payload) | 0.429 ms | 0.827 ms | **-48.17%** |

- The payload microbenchmark runs against in-memory channels; real transports
  amortize this cost across MTU-sized ciphertexts and routing latencies.  The
  aggregated “end-to-end” number stays well under the `< 10%` overhead target,
  even when evaluating both control-plane (handshake) and data-plane (seal/open)
  costs together.
- TLS measurements use rustls 0.21 with an in-memory pipe to eliminate IO noise.
- QSTP numbers include the TupleChain encryption, adaptive routing bookkeeping,
  and deterministic AES-GCM derivations per frame.
- All QSTP perf runs are executed inside the AWRE (wasm-micro-runtime) stack with WAVEN dual page tables enabled and `qrng_feed` entropy seeded through the wazero harness. That way the runtime posture, interpreter/AOT/JIT tiering, and ABW34 telemetry match what DAO-governed deployments enforce.
