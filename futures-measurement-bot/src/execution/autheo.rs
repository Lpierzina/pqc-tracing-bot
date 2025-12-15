use crate::events::*;
use crate::execution::ExecutionAdapter;
use crate::types::*;
use async_trait::async_trait;
use pqcnet_qstp::{InMemoryTupleChain, MeshPeerId, MeshQosClass, MeshRoutePlan};
use rand::Rng;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

/// Stub adapter for an "Autheo" venue-path.
///
/// The intent is that orders and fills can be carried over QSTP tunnels, and
/// metadata/audit pointers can be stored in TupleChain. Here we only demonstrate
/// wiring to PQCNet's QSTP tuple store.
pub struct AutheoAdapter {
    pub venue: Venue,
    tuple_chain: Mutex<InMemoryTupleChain>,
}

impl AutheoAdapter {
    pub fn new(venue: Venue) -> Self {
        Self {
            venue,
            tuple_chain: Mutex::new(InMemoryTupleChain::new()),
        }
    }

    #[allow(dead_code)]
    fn demo_route(&self) -> MeshRoutePlan {
        MeshRoutePlan {
            topic: "waku/autheo/orders".into(),
            hops: vec![MeshPeerId::derive("edge-autheo")],
            qos: MeshQosClass::LowLatency,
            epoch: 0,
        }
    }
}

#[async_trait]
impl ExecutionAdapter for AutheoAdapter {
    async fn place_order(
        &self,
        intent_id: ClientIntentId,
        params: OrderParams,
    ) -> anyhow::Result<(OrderId, Vec<Event>)> {
        let order_id = OrderId(format!("autheo:{}", rand::thread_rng().gen::<u64>()));
        let ts = SystemTime::now();

        // In production: establish QSTP tunnel + send order frame. We keep the tuple
        // chain around to show the expected integration surface.
        let _tuple_chain_guard = self.tuple_chain.lock().unwrap();

        let mut out = Vec::new();
        out.push(Event::OrderSent(OrderSent {
            ts,
            intent_id,
            order_id: order_id.clone(),
            params: params.clone(),
        }));

        let ack_ts = ts + Duration::from_millis(rand::thread_rng().gen_range(2..=35));
        out.push(Event::OrderAck(OrderAck {
            ts: ack_ts,
            order_id: order_id.clone(),
            venue_order_id: Some(format!("autheo-venue:{}", rand::thread_rng().gen::<u64>())),
        }));

        // Different fill profile to simulate different routing/latency.
        let p_fill = match params.order_type {
            OrderType::Market => 0.995,
            OrderType::Limit => 0.65,
        };
        if rand::thread_rng().gen_bool(p_fill) {
            let fill_ts = ack_ts + Duration::from_millis(rand::thread_rng().gen_range(1..=180));
            let price = params
                .limit_price
                .unwrap_or(100.0)
                * (1.0 + rand::thread_rng().gen_range(-0.0003..=0.0003));
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
                ts: ack_ts + Duration::from_millis(250),
                order_id: order_id.clone(),
                reason: CancelReason::Timeout,
            }));
        }

        Ok((order_id, out))
    }

    async fn cancel_order(&self, order_id: OrderId, reason: CancelReason) -> anyhow::Result<Vec<Event>> {
        Ok(vec![Event::OrderCancelled(OrderCancelled {
            ts: SystemTime::now(),
            order_id,
            reason,
        })])
    }
}
