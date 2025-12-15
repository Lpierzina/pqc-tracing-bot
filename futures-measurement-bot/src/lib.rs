//! Futures trading measurement bot primitives.
//!
//! This crate focuses on *execution-quality measurement* rather than PnL alpha.
//! It ingests intent/order/fill/market-data events and computes metrics such as:
//! fill probability, slippage, rejection rate, latency breakdowns, and
//! microstructure response.

pub mod audit;
pub mod buckets;
pub mod events;
pub mod execution;
pub mod metrics;
pub mod strategy;
pub mod types;

pub use crate::metrics::engine::{MetricsConfig, MetricsEngine};
