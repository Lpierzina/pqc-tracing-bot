#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

mod types;
pub use types::{
    KyberEncapsulation, KyberError, KyberKeyPair, KyberLevel, KyberResult,
};

#[cfg(feature = "deterministic")]
mod deterministic;
#[cfg(feature = "deterministic")]
pub use deterministic::KyberDeterministic;

#[cfg(all(feature = "liboqs", not(target_arch = "wasm32")))]
mod liboqs;
#[cfg(all(feature = "liboqs", not(target_arch = "wasm32")))]
pub use liboqs::{KyberAlgorithm, KyberLibOqs};

#[cfg(all(feature = "liboqs", target_arch = "wasm32"))]
compile_error!(
    "The `liboqs` feature is not supported on wasm32 targets. Disable `liboqs` and enable the `deterministic` fallback."
);
