use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Venue(pub String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Symbol(pub String);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Side {
    Buy,
    Sell,
}

impl Side {
    pub fn sign(self) -> f64 {
        match self {
            Side::Buy => 1.0,
            Side::Sell => -1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum OrderType {
    Market,
    Limit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum TimeInForce {
    Ioc,
    Fok,
    Gtc,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrderParams {
    pub symbol: Symbol,
    pub venue: Venue,
    pub side: Side,
    pub order_type: OrderType,
    pub tif: TimeInForce,
    pub qty_contracts: f64,
    pub limit_price: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct StrategyId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct OrderId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ClientIntentId(pub String);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RejectReason {
    InvalidParams,
    RiskLimit,
    MinSize,
    RateLimit,
    ExchangeError,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CancelReason {
    UserRequest,
    Timeout,
    Replace,
    Risk,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FillRole {
    Maker,
    Taker,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Fill {
    pub price: f64,
    pub qty_contracts: f64,
    pub role: FillRole,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BookLevel {
    pub price: f64,
    pub qty: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrderBookTopN {
    pub bids: Vec<BookLevel>,
    pub asks: Vec<BookLevel>,
}

impl OrderBookTopN {
    pub fn best_bid(&self) -> Option<f64> {
        self.bids.first().map(|l| l.price)
    }

    pub fn best_ask(&self) -> Option<f64> {
        self.asks.first().map(|l| l.price)
    }

    pub fn mid(&self) -> Option<f64> {
        Some((self.best_bid()? + self.best_ask()?) / 2.0)
    }

    pub fn spread(&self) -> Option<f64> {
        Some(self.best_ask()? - self.best_bid()?)
    }

    pub fn depth_qty_top_n(&self) -> (f64, f64) {
        let bid = self.bids.iter().map(|l| l.qty).sum();
        let ask = self.asks.iter().map(|l| l.qty).sum();
        (bid, ask)
    }
}
