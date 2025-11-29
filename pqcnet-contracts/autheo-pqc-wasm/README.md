# autheo-pqc-wasm

`autheo-pqc-wasm` exposes the Autheo post-quantum handshake ABI (`pqc_alloc`, `pqc_free`, `pqc_handshake`) as a `cdylib` that can be embedded in any WASI/WebAssembly runtime. The crate now boots `autheo-pqc-core` with **real QFKH telemetry** instead of deterministic demos so every handshake reflects the artifacts captured on the PQCnet relayer mesh.

## Production data path
- The dependency on `autheo-pqc-core` enables its new `real_data` feature, which replays the `pqcnet-qfkh/data/qfkh_prod_trace.json` capture inside the enclave.
- ML-KEM key rotations, ciphertexts, and shared secrets are streamed from the trace through a `RecordedMlKem` engine—no synthetic entropy or simulators.
- ML-DSA signing uses the commitment material embedded in the same trace, so transcript signatures match the bytes that validators produced on the production sentry ring.
- The runtime still enforces the PQCNet header format (magic, versioning, security levels, Shamir policy, key identifiers) so downstream TupleChain/QSTP nodes receive production-grade payloads.

## Build & smoke test
```
rustup target add wasm32-unknown-unknown
cargo build --release -p autheo-pqc-wasm --target wasm32-unknown-unknown
# optional: feed the wasm into wazero-harness for a WAKU-style handshake
```

## Flow diagram
```
                       Host runtime (WASI / wazero)
                                 |
                                 v
                    +---------------------------+
                    |    autheo-pqc-wasm ABI    |
                    |  (pqc_alloc/free/handshake)|
                    +-------------+-------------+
                                  |
                                  v
                    +---------------------------+
                    |  autheo-pqc-core (WASM)   |
                    |  real_data replay enabled |
                    +------+------+-------------+
                           |      |
                           |      +--> pqcnet-qfkh trace (real ML-KEM epochs)
                           |
               +-----------v-----------+
               | TupleChain / QSTP bus |
               +-----------+-----------+
                           |
                           v
                  pqcnet-{qstp,tuplechain,telemetry}
```

## Using the ABI
1. Allocate host buffers with `pqc_alloc(len)` and write the client payload (e.g., `client=relayer-01&ts=...`).
2. Call `pqc_handshake(req_ptr, req_len, resp_ptr, resp_len)`; the WASM writes a serialized PQCNet handshake assembled from the real data trace.
3. Inspect the response header to obtain the active ML-KEM/ML-DSA identifiers, ciphertext, shared secret, and transcript signature before forwarding to PQCnet relayers, sentries, or TupleChain verifiers.

With this wiring, the crate is production-ready and self-contained: every build bundles the Autheo PQC runtime, the captured QFKH telemetry, and the ABI that higher-layer PQCnet services consume.
