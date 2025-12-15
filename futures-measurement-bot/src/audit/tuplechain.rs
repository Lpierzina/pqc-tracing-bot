use crate::audit::{AuditEvent, AuditSink};
use anyhow::Context;
use autheo_pqcnet_tuplechain::{ProofScheme, TupleChainConfig, TupleChainKeeper, TuplePayload};
use serde_json::json;
use std::sync::Mutex;

/// Stores audit events as TupleChain tuples.
///
/// This gives you:
/// - immutable-ish commitments (per tuple version)
/// - shard assignment
/// - simple queryability via subject/predicate/object
pub struct TupleChainAuditSink {
    creator: String,
    keeper: Mutex<TupleChainKeeper>,
}

impl TupleChainAuditSink {
    pub fn new(creator: impl Into<String>) -> Self {
        let creator = creator.into();
        let keeper = TupleChainKeeper::new(TupleChainConfig::default()).allow_creator(creator.clone());
        Self {
            creator,
            keeper: Mutex::new(keeper),
        }
    }
}

impl AuditSink for TupleChainAuditSink {
    fn emit(&self, event: AuditEvent) -> anyhow::Result<()> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let (subject, predicate, object) = match &event {
            AuditEvent::Intent { intent, bucket } => (
                format!("did:bot:{}", intent.strategy_id.0),
                "intent".to_string(),
                json!({"bucket": bucket, "intent": intent}),
            ),
            AuditEvent::Sent { sent } => (
                "did:bot:execution".to_string(),
                "order_sent".to_string(),
                json!({"sent": sent}),
            ),
            AuditEvent::Ack { ack, bucket } => (
                "did:bot:execution".to_string(),
                "order_ack".to_string(),
                json!({"bucket": bucket, "ack": ack}),
            ),
            AuditEvent::Reject { rejected, bucket } => (
                "did:bot:execution".to_string(),
                "order_reject".to_string(),
                json!({"bucket": bucket, "reject": rejected}),
            ),
            AuditEvent::Cancel { cancel, bucket } => (
                "did:bot:execution".to_string(),
                "order_cancel".to_string(),
                json!({"bucket": bucket, "cancel": cancel}),
            ),
            AuditEvent::Fill {
                fill,
                bucket,
                vwap,
                reference_price,
            } => (
                "did:bot:execution".to_string(),
                "order_fill".to_string(),
                json!({"bucket": bucket, "fill": fill, "vwap": vwap, "reference_price": reference_price}),
            ),
            AuditEvent::Timeout {
                ts,
                strategy_id,
                intent_id,
                order_id,
                bucket,
                timeout_ms,
            } => (
                format!("did:bot:{}", strategy_id.0),
                "timeout".to_string(),
                json!({"ts": ts, "intent_id": intent_id, "order_id": order_id, "bucket": bucket, "timeout_ms": timeout_ms}),
            ),
        };

        let bytes = serde_json::to_vec(&object).context("serialize tuple object")?;

        let tuple = TuplePayload::builder(subject, predicate)
            .object_value(object)
            .proof(ProofScheme::Signature, &bytes, "measurement-bot")
            .expiry(u64::MAX)
            .build();

        let mut keeper = self.keeper.lock().unwrap();
        let _receipt = keeper
            .store_tuple(&self.creator, tuple, now_ms)
            .context("store tuple")?;

        Ok(())
    }
}
