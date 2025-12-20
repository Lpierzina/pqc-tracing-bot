//! Strategy layer (signals) placeholder.
//!
//! This repo's focus is measurement. In production, your signal engine will
//! publish `Event::StrategyIntent` and then hand off to an execution adapter.

pub mod distressed_position_rescue;
pub mod open_ma_trend;
