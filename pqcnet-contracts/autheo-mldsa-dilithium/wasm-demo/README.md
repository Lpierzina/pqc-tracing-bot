Streamlined Plan for PQC WASM Module
The PQC WASM module will focus exclusively on ML-KEM (Kyber) and ML-DSA (Dilithium), providing WASM-compatible bindings for key generation, encapsulation/decapsulation (ML-KEM), and signing/verification (ML-DSA). It will not include:

Classical cryptography (ECDSA/ECDH).
Compression (e.g., zstd).
Hashes (e.g., SHA-256).
Networking.
FHE, MPC, bloom filters, or multi-signature schemes.
Threshold cryptography (e.g., Feldman’s VSS or Pedermen VSS).

These will be handled by separate WASM modules, integrated at the application level (e.g., in your quantum-secure enclave or browser-based DiD system).
1. PQC WASM Module Scope
Crate Dependencies:
ml-kem (https://github.com/RustCrypto/KEMs) for ML-KEM.
dilithium2 (https://github.com/Quantum-Blockchains/dilithium) for ML-DSA (Dilithium2, adjustable for Dilithium3/5).
https://crates.io/crates/ml-dsa

Functionality:
ML-KEM:
Key pair generation (keypair).
Encapsulation (encapsulate).
Decapsulation (decapsulate).
Types: EncapsulationKey, DecapsulationKey, Ciphertext, SharedSecret.

ML-DSA:
Key pair generation (generate).
Signing (sign).
Verification (verify).
Types: Keypair, PublicKey, Signature.

WASM Requirements:
wasm_bindgen for JavaScript interop.
serde for serialization/deserialization.
getrandom for WASM-compatible randomness (integratable with enclave QRNG).
zeroize for secure memory handling.

Output: A single WASM module (pqc.wasm) exposing ML-KEM and ML-DSA functions, with wrapped types for WASM compatibility.

