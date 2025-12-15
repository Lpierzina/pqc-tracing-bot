use async_trait::async_trait;
use crate::events::Event;
use crate::types::{CancelReason, ClientIntentId, OrderId, OrderParams};

pub mod trading_station;
pub mod autheo;

#[async_trait]
pub trait ExecutionAdapter: Send + Sync {
    async fn place_order(
        &self,
        intent_id: ClientIntentId,
        params: OrderParams,
    ) -> anyhow::Result<(OrderId, Vec<Event>)>;

    async fn cancel_order(&self, order_id: OrderId, reason: CancelReason) -> anyhow::Result<Vec<Event>>;
}
