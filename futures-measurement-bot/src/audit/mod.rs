use crate::buckets::BucketKey;
use crate::events::{OrderAck, OrderCancelled, OrderFill, OrderRejected, OrderSent, StrategyIntent};
use crate::types::{ClientIntentId, OrderId, StrategyId};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AuditEvent {
    Intent {
        intent: StrategyIntent,
        bucket: BucketKey,
    },
    Sent {
        sent: OrderSent,
    },
    Ack {
        ack: OrderAck,
        bucket: BucketKey,
    },
    Reject {
        rejected: OrderRejected,
        bucket: Option<BucketKey>,
    },
    Cancel {
        cancel: OrderCancelled,
        bucket: BucketKey,
    },
    Fill {
        fill: OrderFill,
        bucket: BucketKey,
        vwap: Option<f64>,
        reference_price: Option<f64>,
    },
    Timeout {
        ts: SystemTime,
        strategy_id: StrategyId,
        intent_id: ClientIntentId,
        order_id: OrderId,
        bucket: BucketKey,
        timeout_ms: u64,
    },
}

impl AuditEvent {
    pub fn intent(intent: StrategyIntent, bucket: BucketKey) -> Self {
        Self::Intent { intent, bucket }
    }

    pub fn sent(sent: OrderSent) -> Self {
        Self::Sent { sent }
    }

    pub fn ack(ack: OrderAck, bucket: BucketKey) -> Self {
        Self::Ack { ack, bucket }
    }

    pub fn reject(rejected: OrderRejected, bucket: Option<BucketKey>) -> Self {
        Self::Reject { rejected, bucket }
    }

    pub fn cancel(cancel: OrderCancelled, bucket: BucketKey) -> Self {
        Self::Cancel { cancel, bucket }
    }

    pub fn fill(fill: OrderFill, bucket: BucketKey, vwap: Option<f64>, reference_price: Option<f64>) -> Self {
        Self::Fill { fill, bucket, vwap, reference_price }
    }

    pub fn timeout(
        ts: SystemTime,
        strategy_id: StrategyId,
        intent_id: ClientIntentId,
        order_id: OrderId,
        bucket: BucketKey,
        timeout_ms: u64,
    ) -> Self {
        Self::Timeout {
            ts,
            strategy_id,
            intent_id,
            order_id,
            bucket,
            timeout_ms,
        }
    }
}

pub trait AuditSink: Send + Sync {
    fn emit(&self, event: AuditEvent) -> anyhow::Result<()>;
}

pub mod tuplechain;
pub mod qsdg;

/// No-op audit sink (useful for demos and web UI).
#[derive(Clone, Debug, Default)]
pub struct NoopAuditSink;

impl AuditSink for NoopAuditSink {
    fn emit(&self, _event: AuditEvent) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Fan-out sink for emitting to multiple audit backends.
pub struct CompositeAuditSink {
    sinks: Vec<std::sync::Arc<dyn AuditSink>>,
}

impl CompositeAuditSink {
    pub fn new(sinks: Vec<std::sync::Arc<dyn AuditSink>>) -> Self {
        Self { sinks }
    }
}

impl AuditSink for CompositeAuditSink {
    fn emit(&self, event: AuditEvent) -> anyhow::Result<()> {
        for sink in &self.sinks {
            sink.emit(event.clone())?;
        }
        Ok(())
    }
}
