#![cfg_attr(target_arch = "wasm32", no_std)]

//! PQCNet contract primitives for ML-KEM, ML-DSA, and QS-DAG anchoring.
//!
//! The crate provides thin wrappers that plug audited post-quantum cryptography
//! engines into PQCNet contract logic. It targets `wasm32` environments that
//! require `no_std`, while remaining compatible with native testing.

extern crate alloc;

pub mod adapters;
pub mod dsa;
pub mod error;
pub mod handshake;
pub mod kem;
pub mod key_manager;
pub mod qs_dag;
pub(crate) mod runtime;
pub mod secret_sharing;
pub mod signatures;
pub mod types;

#[cfg(all(feature = "liboqs", not(target_arch = "wasm32")))]
pub mod liboqs;

#[cfg(all(feature = "liboqs", target_arch = "wasm32"))]
compile_error!("The `liboqs` feature cannot be enabled for wasm32 targets. Disable the feature when building WASM artifacts.");
