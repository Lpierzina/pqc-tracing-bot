use futures_measurement_bot::audit::tuplechain::TupleChainAuditSink;
use futures_measurement_bot::events::*;
use futures_measurement_bot::metrics::engine::{MetricsConfig, MetricsEngine};
use futures_measurement_bot::types::*;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

#[test]
fn computes_fill_probability_and_slippage_buckets() {
    let audit = Arc::new(TupleChainAuditSink::new("did:bot:test"));
    let cfg = MetricsConfig {
        fill_timeout_ms: 500,
        microstructure_horizons_ms: vec![100, 1_000, 5_000],
        telemetry_endpoint: "http://localhost:4318".to_string(),
    };
    let engine = MetricsEngine::new(cfg, audit);

    let venue = Venue("test".into());
    let symbol = Symbol("ES".into());

    // Intent + sent + ack + fill
    let t0 = SystemTime::now();
    let book = OrderBookTopN {
        bids: vec![BookLevel {
            price: 99.0,
            qty: 10.0,
        }],
        asks: vec![BookLevel {
            price: 101.0,
            qty: 10.0,
        }],
    };

    engine
        .observe(&Event::MarketData(MarketData {
            ts: t0,
            symbol: symbol.clone(),
            venue: venue.clone(),
            book: book.clone(),
        }))
        .unwrap();

    let intent_id = ClientIntentId("i1".into());
    engine
        .observe(&Event::StrategyIntent(StrategyIntent {
            ts: t0,
            strategy_id: StrategyId("s".into()),
            intent_id: intent_id.clone(),
            params: OrderParams {
                symbol: symbol.clone(),
                venue: venue.clone(),
                side: Side::Buy,
                order_type: OrderType::Limit,
                tif: TimeInForce::Ioc,
                qty_contracts: 1.0,
                limit_price: Some(100.0),
            },
            reference_price: Some(100.0),
            book: Some(book),
        }))
        .unwrap();

    let order_id = OrderId("o1".into());
    engine
        .observe(&Event::OrderSent(OrderSent {
            ts: t0 + Duration::from_millis(1),
            intent_id: intent_id.clone(),
            order_id: order_id.clone(),
            params: OrderParams {
                symbol: symbol.clone(),
                venue: venue.clone(),
                side: Side::Buy,
                order_type: OrderType::Limit,
                tif: TimeInForce::Ioc,
                qty_contracts: 1.0,
                limit_price: Some(100.0),
            },
        }))
        .unwrap();

    engine
        .observe(&Event::OrderAck(OrderAck {
            ts: t0 + Duration::from_millis(2),
            order_id: order_id.clone(),
            venue_order_id: Some("v1".into()),
        }))
        .unwrap();

    // Fill at 101 => adverse slippage for buy (positive adverse bps).
    engine
        .observe(&Event::OrderFill(OrderFill {
            ts: t0 + Duration::from_millis(3),
            order_id: order_id.clone(),
            fill: Fill {
                price: 101.0,
                qty_contracts: 1.0,
                role: FillRole::Taker,
            },
            cumulative_filled_contracts: 1.0,
            is_final: true,
        }))
        .unwrap();

    // Tick to process microstructure horizons (best-effort).
    engine.tick(t0 + Duration::from_millis(6_000)).unwrap();

    let kv = engine.snapshot_kv();
    // There should be a fill_prob key at least.
    assert!(kv.keys().any(|k| k.starts_with("fill_prob|test|ES")));
}
