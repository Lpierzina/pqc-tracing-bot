#![cfg_attr(target_arch = "wasm32", no_std)]

//! PQCNet contract primitives for ML-KEM, ML-DSA, and QS-DAG anchoring.
//!
//! The crate provides thin wrappers that plug audited post-quantum cryptography
//! engines into PQCNet contract logic. It targets `wasm32` environments that
//! require `no_std`, while remaining compatible with native testing.

extern crate alloc;

#[cfg(target_arch = "wasm32")]
extern crate wee_alloc;

#[cfg(target_arch = "wasm32")]
use core::panic::PanicInfo;
#[cfg(target_arch = "wasm32")]
use wee_alloc::WeeAlloc;

#[cfg(target_arch = "wasm32")]
mod wasm;

pub mod dsa;
pub mod error;
pub mod handshake;
pub mod kem;
pub mod key_manager;
pub mod qs_dag;
pub mod signatures;
pub mod types;

#[cfg(target_arch = "wasm32")]
#[global_allocator]
static ALLOC: WeeAlloc = WeeAlloc::INIT;

#[cfg(target_arch = "wasm32")]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
