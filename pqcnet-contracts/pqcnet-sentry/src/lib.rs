//! Library surface for the `pqcnet-sentry` binary.
//!
//! The binary re-exports its config parser and service logic so doctests and
//! runnable examples can link against them. This keeps the CLI thin while
//! letting other workspace crates (and future standalone repos) reuse the
//! sentry logic as a regular library.

pub mod config;
pub mod service;
