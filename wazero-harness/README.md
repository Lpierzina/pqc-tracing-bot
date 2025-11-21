# wazero-harness

`wazero-harness` is a Go integration harness that loads the `autheo_pqc_wasm` artifact with [wazero](https://github.com/tetratelabs/wazero), calls the exported `pqc_handshake` ABI, parses the PQC1 envelope, persists advertised key metadata, and simulates the QS-DAG anchoring flow. It is the fastest way to prove that the compiled PQCNet contract can be invoked from a host runtime without touching Rust.

## Prerequisites

- Go 1.22+ (the module targets 1.22 in `go.mod`)
- A Rust toolchain with the `wasm32-unknown-unknown` target installed
- A built `autheo_pqc_wasm.wasm` binary (see next section)

## Build the WASM contract artifact

```bash
cd pqcnet-contracts
rustup target add wasm32-unknown-unknown # run once
cargo build --release -p autheo-pqc-wasm --target wasm32-unknown-unknown
# => target/wasm32-unknown-unknown/release/autheo_pqc_wasm.wasm
```

> Tip: pass `--features liboqs` to `cargo build` if you want the WASM to link against Autheo’s liboqs-backed Kyber/Dilithium engines instead of the deterministic demo adapters.

## Run the harness

```bash
cd wazero-harness
go run . \
  -wasm ../pqcnet-contracts/target/wasm32-unknown-unknown/release/autheo_pqc_wasm.wasm
```

The harness will:

1. Build a deterministic handshake payload (`client=autheo-demo&ts=<unix-nanos>`) and derive a DAG `edge_id`.
2. Allocate request/response buffers with `pqc_alloc` and invoke `pqc_handshake`.
3. Parse the PQC1 binary envelope, persist the announced KEM and DSA keys (including rotation metadata and thresholds), and recompute the transcript signature.
4. Re-verify the signature against the stored payload, mimicking `pqcnet-qs-dag::QsDagPqc::verify_and_anchor`.

A successful run prints a line similar to:

```
Handshake OK → kem_key=<hex> signer=<hex> t=3/5 ciphertext=1536B shared=32B signature=32B
QS-DAG anchor stored for edge=<id> signer=<hex>
```

Use `-wasm <path>` to point at alternate builds, regression candidates, or fuzzed WASM modules.

## How to test

- `go test ./...` — compiles the harness and catches regressions in helper functions (there are no long-running tests, so this finishes quickly).
- `go vet ./...` — optional static analysis to keep host-side memory handling sound.
- `go test ./... && go run . -wasm <path>` — canonical integration smoke test: build the WASM, run Go tests, then exercise the handshake end-to-end.
- For deeper coverage, pair the harness run with `cargo test -p autheo-pqc-core qstp::tests::qstp_rerouted_payload_decrypts` to confirm the same contract logic succeeds under Rust integration tests.

## PQCNet contract API schema

The harness consumes the same contract that external PQCNet clients bind to. For convenience, the full protobuf schema (`pqcnet-contracts/protos/qstp.proto`) is reproduced below so you can inspect every RPC surface and message layout alongside the harness output. `OpenTunnel` corresponds to the `pqc_handshake` envelope, `PublishFrame` transports AES-256-GCM payloads, and `ReportQace` carries adaptive routing signals.

```proto
syntax = "proto3";

package pqcnet.qstp;

option go_package = "github.com/autheo-one/pqcnet/qstp/proto";

enum MeshQosClass {
  MESH_QOS_UNKNOWN = 0;
  MESH_QOS_GOSSIP = 1;
  MESH_QOS_LOW_LATENCY = 2;
  MESH_QOS_CONTROL = 3;
}

enum QaceAction {
  QACE_ACTION_MAINTAIN = 0;
  QACE_ACTION_REKEY = 1;
  QACE_ACTION_REROUTE = 2;
}

message MeshRoutePlan {
  string topic = 1;
  repeated bytes hops = 2;
  MeshQosClass qos = 3;
  uint64 epoch = 4;
}

message HandshakeInit {
  bytes route_hash = 1;
  bytes ciphertext = 2;
  bytes initiator_nonce = 3;
  bytes client_signature = 4;
  bytes client_signing_key = 5;
  bytes client_signing_key_id = 6;
  bytes server_signing_key_id = 7;
  bytes application_data = 8;
}

message HandshakeResponse {
  bytes route_hash = 1;
  bytes session_id = 2;
  bytes responder_nonce = 3;
  bytes responder_signature = 4;
  bytes server_signing_key = 5;
  bytes server_signing_key_id = 6;
  bytes server_kem_key = 7;
  bytes server_kem_key_id = 8;
}

message SessionKeyMaterial {
  bytes send_key = 1;
  bytes send_nonce = 2;
  bytes recv_key = 3;
  bytes recv_nonce = 4;
  bytes tuple_key = 5;
  bytes session_id = 6;
}

message TupleMetadata {
  bytes tunnel_id = 1;
  bytes kem_key_id = 2;
  bytes signing_key_id = 3;
  uint32 threshold_t = 4;
  uint32 threshold_n = 5;
  bytes route_hash = 6;
  MeshQosClass qos = 7;
  uint64 route_epoch = 8;
  uint64 established_at = 9;
  bytes tuple_pointer = 10;
}

message HandshakeEnvelope {
  HandshakeInit init = 1;
  HandshakeResponse response = 2;
  TupleMetadata metadata = 3;
}

message QstpFrame {
  bytes tunnel_id = 1;
  uint64 sequence = 2;
  bytes nonce = 3;
  bytes ciphertext = 4;
  bytes route_hash = 5;
  uint64 route_epoch = 6;
  string topic = 7;
}

message QaceSignal {
  uint32 latency_ms = 1;
  uint32 loss_bps = 2;
  uint32 threat_score = 3;
  uint32 route_changes = 4;
}

message QaceDecision {
  QaceAction action = 1;
  MeshRoutePlan new_route = 2;
  uint32 score = 3;
  string rationale = 4;
}

message OpenTunnelRequest {
  bytes client_request = 1;
  MeshRoutePlan preferred_route = 2;
  bytes peer_id = 3;
}

message OpenTunnelResponse {
  HandshakeEnvelope envelope = 1;
  SessionKeyMaterial session = 2;
}

message PublishFrameRequest {
  QstpFrame frame = 1;
}

message FrameAck {
  bytes tunnel_id = 1;
  uint64 sequence = 2;
}

message QaceReport {
  bytes tunnel_id = 1;
  QaceSignal signal = 2;
}

service QstpMesh {
  rpc OpenTunnel(OpenTunnelRequest) returns (OpenTunnelResponse);
  rpc PublishFrame(PublishFrameRequest) returns (FrameAck);
  rpc ReportQace(QaceReport) returns (QaceDecision);
}
```

Use these definitions when wiring external clients, regenerating stubs, or validating harness output against automated tests. Keeping the schema alongside the harness avoids drift between the WASM ABI, the gRPC interfaces, and any future Waku/THEO adapters.
