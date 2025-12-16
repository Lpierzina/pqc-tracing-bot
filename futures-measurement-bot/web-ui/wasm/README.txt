This folder is served at /wasm/ by the demo server.

Expected file for the UI:
- autheo_pqc_wasm.wasm

Build it with:
  cargo build --release -p autheo-pqc-wasm --target wasm32-unknown-unknown

Then copy from:
  pqcnet-contracts/target/wasm32-unknown-unknown/release/autheo_pqc_wasm.wasm

into this folder (or let the server serve it directly from that build output if present).
