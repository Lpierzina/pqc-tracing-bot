use crate::types::{OrderType, Side, Symbol, Venue};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum QtyBucket {
    Micro,
    Small,
    Medium,
    Large,
}

impl QtyBucket {
    pub fn from_qty_contracts(qty: f64) -> Self {
        if qty <= 0.25 {
            QtyBucket::Micro
        } else if qty <= 1.0 {
            QtyBucket::Small
        } else if qty <= 5.0 {
            QtyBucket::Medium
        } else {
            QtyBucket::Large
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum TodBucket {
    /// 00:00-06:00
    Asia,
    /// 06:00-12:00
    Europe,
    /// 12:00-18:00
    Us,
    /// 18:00-24:00
    Late,
}

impl TodBucket {
    pub fn from_hour_utc(hour: u32) -> Self {
        match hour {
            0..=5 => TodBucket::Asia,
            6..=11 => TodBucket::Europe,
            12..=17 => TodBucket::Us,
            _ => TodBucket::Late,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BucketKey {
    pub symbol: Symbol,
    pub venue: Venue,
    pub side: Side,
    pub order_type: OrderType,
    pub qty_bucket: QtyBucket,
    pub tod: TodBucket,
}
