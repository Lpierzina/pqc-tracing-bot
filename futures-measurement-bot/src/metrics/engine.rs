use crate::buckets::{BucketKey, QtyBucket, TodBucket};
use crate::events::*;
use crate::metrics::stats::BucketStats;
use crate::types::*;
use anyhow::Context;
use chrono::{DateTime, Timelike, Utc};
use dashmap::DashMap;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use pqcnet_telemetry::{TelemetryConfig, TelemetryHandle};

use crate::audit::{AuditEvent, AuditSink};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MetricsConfig {
    pub fill_timeout_ms: u64,
    pub microstructure_horizons_ms: Vec<u64>,
    pub telemetry_endpoint: String,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            fill_timeout_ms: 2_000,
            microstructure_horizons_ms: vec![100, 1_000, 5_000],
            telemetry_endpoint: "http://localhost:4318".to_string(),
        }
    }
}

#[derive(Clone, Debug)]
struct OrderState {
    bucket: BucketKey,
    strategy_id: StrategyId,
    intent_id: ClientIntentId,
    order_id: OrderId,
    params: OrderParams,

    t_decision: SystemTime,
    t_send: Option<SystemTime>,
    t_ack: Option<SystemTime>,
    t_first_fill: Option<SystemTime>,
    t_last_fill: Option<SystemTime>,

    reference_price: Option<f64>,
    decision_book: Option<OrderBookTopN>,

    filled_qty: f64,
    vwap_numer: f64,
    terminal: bool,
}

impl OrderState {
    fn vwap(&self) -> Option<f64> {
        if self.filled_qty <= 0.0 {
            return None;
        }
        Some(self.vwap_numer / self.filled_qty)
    }
}

#[derive(Clone)]
pub struct MetricsEngine {
    cfg: MetricsConfig,
    telemetry: TelemetryHandle,
    audit: Arc<dyn AuditSink>,

    by_order: DashMap<OrderId, OrderState>,
    /// Latest market snapshots by (venue,symbol).
    books: DashMap<(Venue, Symbol), (SystemTime, OrderBookTopN)>,

    /// Bucket stats are mutated frequently; a per-bucket mutex keeps it simple.
    buckets: DashMap<BucketKey, Arc<Mutex<BucketStats>>>,

    /// A tiny in-memory QS-DAG-ish key-value summary for quick inspection.
    latest_kv: DashMap<String, String>,

    pending_micro: DashMap<OrderId, PendingMicro>,
}

#[derive(Clone, Debug)]
struct PendingMicro {
    bucket: BucketKey,
    venue: Venue,
    symbol: Symbol,
    mid_before: f64,
    fill_ts: SystemTime,
    remaining: Vec<u64>,
}

impl MetricsEngine {
    pub fn new(cfg: MetricsConfig, audit: Arc<dyn AuditSink>) -> Self {
        let telemetry = TelemetryHandle::from_config(TelemetryConfig::sample(&cfg.telemetry_endpoint));
        Self {
            cfg,
            telemetry,
            audit,
            by_order: DashMap::new(),
            books: DashMap::new(),
            buckets: DashMap::new(),
            latest_kv: DashMap::new(),
            pending_micro: DashMap::new(),
        }
    }

    pub fn telemetry(&self) -> &TelemetryHandle {
        &self.telemetry
    }

    pub fn snapshot_kv(&self) -> BTreeMap<String, String> {
        self.latest_kv
            .iter()
            .map(|kv| (kv.key().clone(), kv.value().clone()))
            .collect()
    }

    pub fn observe(&self, event: &Event) -> anyhow::Result<()> {
        match event {
            Event::MarketData(md) => {
                self.books
                    .insert((md.venue.clone(), md.symbol.clone()), (md.ts, md.book.clone()));
                Ok(())
            }
            Event::StrategyIntent(intent) => self.on_intent(intent),
            Event::OrderSent(sent) => self.on_sent(sent),
            Event::OrderAck(ack) => self.on_ack(ack),
            Event::OrderRejected(rej) => self.on_reject(rej),
            Event::OrderCancelled(can) => self.on_cancel(can),
            Event::OrderFill(fill) => self.on_fill(fill),
        }
    }

    pub fn tick(&self, now: SystemTime) -> anyhow::Result<()> {
        // Detect timeouts: intent+send exists but no terminal outcome by fill_timeout.
        let timeout = Duration::from_millis(self.cfg.fill_timeout_ms);
        for mut entry in self.by_order.iter_mut() {
            if entry.terminal {
                continue;
            }
            let Some(t_send) = entry.t_send else { continue };
            if now.duration_since(t_send).unwrap_or_default() >= timeout {
                entry.terminal = true;
                let stats = self.bucket_stats(&entry.bucket);
                {
                    let mut s = stats.lock();
                    s.fills.timed_out += 1;
                }
                self.telemetry
                    .record_counter("orders.timeout", 1)
                    .context("telemetry")?;

                let audit = AuditEvent::timeout(
                    now,
                    entry.strategy_id.clone(),
                    entry.intent_id.clone(),
                    entry.order_id.clone(),
                    entry.bucket.clone(),
                    self.cfg.fill_timeout_ms,
                );
                self.audit.emit(audit)?;
            }
        }

        // Microstructure horizons: when now >= fill_ts + horizon, record drift using latest book.
        let mut done = Vec::new();
        for pending_ref in self.pending_micro.iter() {
            let order_id = pending_ref.key().clone();
            let pending = pending_ref.value().clone();
            drop(pending_ref);

            let mut remaining = Vec::new();
            for horizon_ms in pending.remaining {
                let target = pending
                    .fill_ts
                    .checked_add(Duration::from_millis(horizon_ms))
                    .unwrap_or(pending.fill_ts);
                if now < target {
                    remaining.push(horizon_ms);
                    continue;
                }

                let key = (pending.venue.clone(), pending.symbol.clone());
                let Some(latest) = self.books.get(&key) else {
                    remaining.push(horizon_ms);
                    continue;
                };
                let (_book_ts, book_after) = latest.value();
                let Some(mid_after) = book_after.mid() else {
                    remaining.push(horizon_ms);
                    continue;
                };
                if pending.mid_before <= 0.0 {
                    remaining.push(horizon_ms);
                    continue;
                }
                let drift_bps = ((mid_after - pending.mid_before) / pending.mid_before) * 10_000.0;

                let stats = self.bucket_stats(&pending.bucket);
                match horizon_ms {
                    100 => stats
                        .lock()
                        .mid_drift_100ms_bps
                        .record((drift_bps.abs() * 100.0) as u64),
                    1_000 => stats
                        .lock()
                        .mid_drift_1s_bps
                        .record((drift_bps.abs() * 100.0) as u64),
                    5_000 => stats
                        .lock()
                        .mid_drift_5s_bps
                        .record((drift_bps.abs() * 100.0) as u64),
                    _ => {}
                }
            }

            if remaining.is_empty() {
                done.push(order_id);
            } else {
                self.pending_micro.insert(
                    order_id,
                    PendingMicro {
                        remaining,
                        ..pending
                    },
                );
            }
        }
        for order_id in done {
            self.pending_micro.remove(&order_id);
        }
        Ok(())
    }

    fn on_intent(&self, intent: &StrategyIntent) -> anyhow::Result<()> {
        let bucket = bucket_for_params(&intent.params);
        let stats = self.bucket_stats(&bucket);

        // microstructure context at decision (if missing, try to fill from latest book).
        let decision_book = intent.book.clone().or_else(|| {
            self.books
                .get(&(intent.params.venue.clone(), intent.params.symbol.clone()))
                .map(|v| v.value().1.clone())
        });

        if let Some(book) = &decision_book {
            if let Some(spread) = book.spread() {
                let mid = book.mid().unwrap_or(0.0);
                if mid > 0.0 {
                    let spread_bps = (spread / mid) * 10_000.0;
                    stats
                        .lock()
                        .spread_bps_at_decision
                        .record((spread_bps.abs() * 100.0) as u64);
                }
            }
            let (bid, ask) = book.depth_qty_top_n();
            stats
                .lock()
                .depth_bid_topn_at_decision
                .record((bid.max(0.0) * 100.0) as u64);
            stats
                .lock()
                .depth_ask_topn_at_decision
                .record((ask.max(0.0) * 100.0) as u64);
        }

        let state = OrderState {
            bucket: bucket.clone(),
            strategy_id: intent.strategy_id.clone(),
            intent_id: intent.intent_id.clone(),
            order_id: OrderId("<unassigned>".to_string()),
            params: intent.params.clone(),
            t_decision: intent.ts,
            t_send: None,
            t_ack: None,
            t_first_fill: None,
            t_last_fill: None,
            reference_price: intent.reference_price,
            decision_book,
            filled_qty: 0.0,
            vwap_numer: 0.0,
            terminal: false,
        };

        // We don’t know OrderId yet; use a synthetic placeholder key and later replace.
        self.by_order
            .insert(OrderId(format!("intent:{}", intent.intent_id.0)), state);

        self.telemetry
            .record_counter("intents.total", 1)
            .context("telemetry")?;
        self.audit.emit(AuditEvent::intent(intent.clone(), bucket))?;

        Ok(())
    }

    fn on_sent(&self, sent: &OrderSent) -> anyhow::Result<()> {
        let placeholder = OrderId(format!("intent:{}", sent.intent_id.0));
        let mut state = self
            .by_order
            .remove(&placeholder)
            .map(|kv| kv.1)
            .with_context(|| format!("missing intent for sent order (intent_id={})", sent.intent_id.0))?;

        state.order_id = sent.order_id.clone();
        state.t_send = Some(sent.ts);
        state.params = sent.params.clone();

        // decision->send latency
        let dt = sent
            .ts
            .duration_since(state.t_decision)
            .unwrap_or_default()
            .as_millis() as u64;
        self.bucket_stats(&state.bucket)
            .lock()
            .latency_decision_to_send_ms
            .record(dt.max(1));
        self.telemetry.record_latency_ms("latency.decision_to_send_ms", dt);
        self.telemetry
            .record_counter("orders.submitted", 1)
            .context("telemetry")?;

        self.bucket_stats(&state.bucket).lock().rejects.submitted += 1;

        self.by_order.insert(sent.order_id.clone(), state);
        self.audit.emit(AuditEvent::sent(sent.clone()))?;
        Ok(())
    }

    fn on_ack(&self, ack: &OrderAck) -> anyhow::Result<()> {
        let mut entry = self
            .by_order
            .get_mut(&ack.order_id)
            .with_context(|| format!("unknown order ack ({})", ack.order_id.0))?;
        entry.t_ack = Some(ack.ts);

        if let Some(t_send) = entry.t_send {
            let dt = ack.ts.duration_since(t_send).unwrap_or_default().as_millis() as u64;
            self.bucket_stats(&entry.bucket)
                .lock()
                .latency_send_to_ack_ms
                .record(dt.max(1));
            self.telemetry.record_latency_ms("latency.send_to_ack_ms", dt);
        }

        self.telemetry
            .record_counter("orders.acked", 1)
            .context("telemetry")?;
        self.audit.emit(AuditEvent::ack(ack.clone(), entry.bucket.clone()))?;
        Ok(())
    }

    fn on_reject(&self, rej: &OrderRejected) -> anyhow::Result<()> {
        // Rejection could arrive with either intent_id or order_id (depends on venue).
        if let Some(order_id) = &rej.order_id {
            if let Some(mut entry) = self.by_order.get_mut(order_id) {
                entry.terminal = true;
                self.bucket_stats(&entry.bucket).lock().rejects.rejected += 1;
                self.telemetry
                    .record_counter("orders.rejected", 1)
                    .context("telemetry")?;
                self.audit.emit(AuditEvent::reject(rej.clone(), Some(entry.bucket.clone())))?;
                return Ok(());
            }
        }

        // Fallback: if intent_id exists, try placeholder.
        if let Some(intent_id) = &rej.intent_id {
            let placeholder = OrderId(format!("intent:{}", intent_id.0));
            if let Some(mut entry) = self.by_order.get_mut(&placeholder) {
                entry.terminal = true;
                self.bucket_stats(&entry.bucket).lock().rejects.rejected += 1;
                self.telemetry
                    .record_counter("orders.rejected", 1)
                    .context("telemetry")?;
                self.audit.emit(AuditEvent::reject(rej.clone(), Some(entry.bucket.clone())))?;
                return Ok(());
            }
        }

        // If we can’t associate, still audit it.
        self.telemetry
            .record_counter("orders.rejected.unattributed", 1)
            .context("telemetry")?;
        self.audit.emit(AuditEvent::reject(rej.clone(), None))?;
        Ok(())
    }

    fn on_cancel(&self, can: &OrderCancelled) -> anyhow::Result<()> {
        let mut entry = self
            .by_order
            .get_mut(&can.order_id)
            .with_context(|| format!("unknown cancel ({})", can.order_id.0))?;
        if entry.terminal {
            return Ok(());
        }
        entry.terminal = true;

        self.bucket_stats(&entry.bucket).lock().fills.cancelled += 1;
        self.telemetry
            .record_counter("orders.cancelled", 1)
            .context("telemetry")?;
        self.audit.emit(AuditEvent::cancel(can.clone(), entry.bucket.clone()))?;
        Ok(())
    }

    fn on_fill(&self, f: &OrderFill) -> anyhow::Result<()> {
        let mut entry = self
            .by_order
            .get_mut(&f.order_id)
            .with_context(|| format!("unknown fill ({})", f.order_id.0))?;

        // Aggregate VWAP.
        entry.filled_qty += f.fill.qty_contracts;
        entry.vwap_numer += f.fill.price * f.fill.qty_contracts;

        if entry.t_first_fill.is_none() {
            entry.t_first_fill = Some(f.ts);
            if let Some(t_send) = entry.t_send {
                let dt = f.ts.duration_since(t_send).unwrap_or_default().as_millis() as u64;
                self.bucket_stats(&entry.bucket)
                    .lock()
                    .latency_send_to_first_fill_ms
                    .record(dt.max(1));
                self.telemetry.record_latency_ms("latency.send_to_first_fill_ms", dt);
            }
        }

        if f.is_final {
            entry.t_last_fill = Some(f.ts);
            entry.terminal = true;
            self.bucket_stats(&entry.bucket).lock().fills.filled += 1;
            self.telemetry
                .record_counter("orders.filled", 1)
                .context("telemetry")?;

            if let Some(t_send) = entry.t_send {
                let dt = f.ts.duration_since(t_send).unwrap_or_default().as_millis() as u64;
                self.bucket_stats(&entry.bucket)
                    .lock()
                    .latency_send_to_last_fill_ms
                    .record(dt.max(1));
                self.telemetry.record_latency_ms("latency.send_to_last_fill_ms", dt);
            }

            // Slippage: compare final VWAP to reference.
            if let (Some(vwap), Some(reference)) = (entry.vwap(), entry.reference_price) {
                if reference > 0.0 {
                    // Define \"adverse\" in strategy sense: buys are worse above reference, sells are worse below reference.
                    let signed_adverse = entry.params.side.sign() * (vwap - reference);
                    let bps = (signed_adverse / reference) * 10_000.0;
                    if bps >= 0.0 {
                        self.bucket_stats(&entry.bucket)
                            .lock()
                            .adverse_slippage_bps
                            .record((bps * 100.0) as u64);
                    } else {
                        self.bucket_stats(&entry.bucket)
                            .lock()
                            .favorable_slippage_bps
                            .record((bps.abs() * 100.0) as u64);
                    }
                }
            }

            // Microstructure response: schedule horizon measurements.
            self.schedule_microstructure(&entry, f.ts);

            // Update quick KV summary for operators.
            let fills = self.bucket_stats(&entry.bucket).lock().fills.clone();
            self.latest_kv.insert(
                format!(
                    "fill_prob|{}|{}|{:?}|{:?}",
                    entry.params.venue.0, entry.params.symbol.0, entry.params.side, entry.params.order_type
                ),
                format!("{:.4}", fills.fill_probability()),
            );
        }

        self.audit.emit(AuditEvent::fill(f.clone(), entry.bucket.clone(), entry.vwap(), entry.reference_price))?;
        Ok(())
    }

    fn schedule_microstructure(&self, entry: &OrderState, fill_ts: SystemTime) {
        let Some(book_before) = &entry.decision_book else { return };
        let Some(mid_before) = book_before.mid() else { return };
        if mid_before <= 0.0 {
            return;
        }

        self.pending_micro.insert(
            entry.order_id.clone(),
            PendingMicro {
                bucket: entry.bucket.clone(),
                venue: entry.params.venue.clone(),
                symbol: entry.params.symbol.clone(),
                mid_before,
                fill_ts,
                remaining: self.cfg.microstructure_horizons_ms.clone(),
            },
        );
    }

    fn bucket_stats(&self, key: &BucketKey) -> Arc<Mutex<BucketStats>> {
        self.buckets
            .entry(key.clone())
            .or_insert_with(|| Arc::new(Mutex::new(BucketStats::default())))
            .clone()
    }
}

fn bucket_for_params(params: &OrderParams) -> BucketKey {
    let qty_bucket = QtyBucket::from_qty_contracts(params.qty_contracts);
    let now: DateTime<Utc> = SystemTime::now().into();
    let tod = TodBucket::from_hour_utc(now.hour());
    BucketKey {
        symbol: params.symbol.clone(),
        venue: params.venue.clone(),
        side: params.side,
        order_type: params.order_type,
        qty_bucket,
        tod,
    }
}
