//! Library facade for the `pqcnet-relayer` binary.
//!
//! Re-exporting the config loader and service logic lets doctests,
//! integration tests, and examples exercise the relayer without going
//! through the CLI entrypoint. This mirrors how the crate will eventually
//! be consumed once it is split into a standalone module.

pub mod config;
pub mod service;
