use crate::events::*;
use crate::execution::ExecutionAdapter;
use crate::types::*;
use async_trait::async_trait;
use rand::Rng;
use std::time::{Duration, SystemTime};

/// Stub adapter for "Trading Station".
///
/// In production this would speak the platform's API (or plugin SDK), and emit
/// raw timestamps for send/ack/fill.
pub struct TradingStationAdapter {
    pub venue: Venue,
}

#[async_trait]
impl ExecutionAdapter for TradingStationAdapter {
    async fn place_order(
        &self,
        intent_id: ClientIntentId,
        params: OrderParams,
    ) -> anyhow::Result<(OrderId, Vec<Event>)> {
        let order_id = OrderId(format!("ts:{}", rand::thread_rng().gen::<u64>()));
        let ts = SystemTime::now();

        // In reality: send to venue; here we simulate ack and probabilistic fill.
        let mut out = Vec::new();
        out.push(Event::OrderSent(OrderSent {
            ts,
            intent_id,
            order_id: order_id.clone(),
            params: params.clone(),
        }));

        let ack_ts = ts + Duration::from_millis(rand::thread_rng().gen_range(1..=20));
        out.push(Event::OrderAck(OrderAck {
            ts: ack_ts,
            order_id: order_id.clone(),
            venue_order_id: Some(format!("venue:{}", rand::thread_rng().gen::<u64>())),
        }));

        // Simulate fill chance.
        let p_fill = match params.order_type {
            OrderType::Market => 0.98,
            OrderType::Limit => 0.55,
        };
        if rand::thread_rng().gen_bool(p_fill) {
            let fill_ts = ack_ts + Duration::from_millis(rand::thread_rng().gen_range(1..=250));
            let price = params.limit_price.unwrap_or(100.0)
                * (1.0 + rand::thread_rng().gen_range(-0.0005..=0.0005));
            out.push(Event::OrderFill(OrderFill {
                ts: fill_ts,
                order_id: order_id.clone(),
                fill: Fill {
                    price,
                    qty_contracts: params.qty_contracts,
                    role: FillRole::Unknown,
                },
                cumulative_filled_contracts: params.qty_contracts,
                is_final: true,
            }));
        } else {
            out.push(Event::OrderCancelled(OrderCancelled {
                ts: ack_ts + Duration::from_millis(300),
                order_id: order_id.clone(),
                reason: CancelReason::Timeout,
            }));
        }

        Ok((order_id, out))
    }

    async fn cancel_order(
        &self,
        order_id: OrderId,
        reason: CancelReason,
    ) -> anyhow::Result<Vec<Event>> {
        Ok(vec![Event::OrderCancelled(OrderCancelled {
            ts: SystemTime::now(),
            order_id,
            reason,
        })])
    }
}
