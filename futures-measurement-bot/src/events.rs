use serde::{Deserialize, Serialize};
use std::time::SystemTime;

use crate::types::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    Intent,
    Send,
    Ack,
    Reject,
    Cancel,
    Fill,
    Book,
}

/// Everything the bot needs to measure execution quality flows through this enum.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Event {
    StrategyIntent(StrategyIntent),
    OrderSent(OrderSent),
    OrderAck(OrderAck),
    OrderRejected(OrderRejected),
    OrderCancelled(OrderCancelled),
    OrderFill(OrderFill),
    MarketData(MarketData),
}

impl Event {
    pub fn kind(&self) -> EventKind {
        match self {
            Event::StrategyIntent(_) => EventKind::Intent,
            Event::OrderSent(_) => EventKind::Send,
            Event::OrderAck(_) => EventKind::Ack,
            Event::OrderRejected(_) => EventKind::Reject,
            Event::OrderCancelled(_) => EventKind::Cancel,
            Event::OrderFill(_) => EventKind::Fill,
            Event::MarketData(_) => EventKind::Book,
        }
    }

    pub fn ts(&self) -> SystemTime {
        match self {
            Event::StrategyIntent(e) => e.ts,
            Event::OrderSent(e) => e.ts,
            Event::OrderAck(e) => e.ts,
            Event::OrderRejected(e) => e.ts,
            Event::OrderCancelled(e) => e.ts,
            Event::OrderFill(e) => e.ts,
            Event::MarketData(e) => e.ts,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StrategyIntent {
    pub ts: SystemTime,
    pub strategy_id: StrategyId,
    pub intent_id: ClientIntentId,
    pub params: OrderParams,
    /// Reference price captured at decision time (mid/bbo/mark). If absent, slippage cannot be computed.
    pub reference_price: Option<f64>,
    /// Optional local book snapshot at decision time (for microstructure response).
    pub book: Option<OrderBookTopN>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderSent {
    pub ts: SystemTime,
    pub intent_id: ClientIntentId,
    pub order_id: OrderId,
    pub params: OrderParams,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderAck {
    pub ts: SystemTime,
    pub order_id: OrderId,
    pub venue_order_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderRejected {
    pub ts: SystemTime,
    pub intent_id: Option<ClientIntentId>,
    pub order_id: Option<OrderId>,
    pub reason: RejectReason,
    pub message: String,
    pub params: Option<OrderParams>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderCancelled {
    pub ts: SystemTime,
    pub order_id: OrderId,
    pub reason: CancelReason,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderFill {
    pub ts: SystemTime,
    pub order_id: OrderId,
    pub fill: Fill,
    pub cumulative_filled_contracts: f64,
    pub is_final: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarketData {
    pub ts: SystemTime,
    pub symbol: Symbol,
    pub venue: Venue,
    pub book: OrderBookTopN,
}
