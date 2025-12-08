//! HQC (NIST ML-KEM backup) bindings for the Autheo PQCNet stack.
//! This crate intentionally ships **only** the production-grade liboqs implementation—no
//! deterministic fallbacks or simulators.

#![forbid(unsafe_code)]

extern crate alloc;

mod types;
pub use types::{HqcEncapsulation, HqcError, HqcKeyPair, HqcLevel, HqcResult};

#[cfg(feature = "liboqs")]
mod liboqs;
#[cfg(feature = "liboqs")]
pub use liboqs::{HqcAlgorithm, HqcLibOqs};

#[cfg(not(feature = "liboqs"))]
compile_error!(
    "autheo-pqcnet-hqc requires the `liboqs` feature. The crate only ships the production liboqs HQC engine—no deterministic fallback is provided."
);
