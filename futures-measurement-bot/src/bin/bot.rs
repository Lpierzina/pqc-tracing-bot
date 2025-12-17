use clap::Parser;
use futures_measurement_bot::audit::qsdg::QsDagAuditSink;
use futures_measurement_bot::audit::tuplechain::TupleChainAuditSink;
use futures_measurement_bot::audit::{AuditSink, CompositeAuditSink};
use futures_measurement_bot::events::*;
use futures_measurement_bot::execution::autheo::AutheoAdapter;
use futures_measurement_bot::execution::trading_station::TradingStationAdapter;
use futures_measurement_bot::execution::ExecutionAdapter;
use futures_measurement_bot::metrics::engine::{MetricsConfig, MetricsEngine};
use futures_measurement_bot::types::*;
use rand::Rng;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

#[derive(Parser, Debug)]
#[command(name = "bot")]
struct Args {
    #[arg(long, default_value = "autheo")]
    venue: String,

    #[arg(long, default_value = "ES")]
    symbol: String,

    #[arg(long, default_value = "buy")]
    side: String,

    #[arg(long, default_value = "limit")]
    order_type: String,

    #[arg(long, default_value_t = 1.0)]
    qty: f64,

    #[arg(long, default_value_t = 50)]
    iters: u32,
}

fn parse_side(s: &str) -> Side {
    match s.to_ascii_lowercase().as_str() {
        "buy" | "long" => Side::Buy,
        _ => Side::Sell,
    }
}

fn parse_order_type(s: &str) -> OrderType {
    match s.to_ascii_lowercase().as_str() {
        "market" => OrderType::Market,
        _ => OrderType::Limit,
    }
}

fn synth_book(mid: f64) -> OrderBookTopN {
    let spread = (mid * 0.0002).max(0.25);
    let bid = mid - spread / 2.0;
    let ask = mid + spread / 2.0;
    OrderBookTopN {
        bids: vec![BookLevel {
            price: bid,
            qty: 50.0,
        }],
        asks: vec![BookLevel {
            price: ask,
            qty: 50.0,
        }],
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
    let progress = std::env::var_os("BOT_PROGRESS").is_some();
    let p = |msg: &str| {
        if progress {
            eprintln!("[bot] {msg}");
        }
    };

    let symbol = Symbol(args.symbol);
    let venue = Venue(args.venue.clone());
    let side = parse_side(&args.side);
    let order_type = parse_order_type(&args.order_type);

    let audit_tuple = Arc::new(TupleChainAuditSink::new("did:bot:futures-measurement"));
    let audit_dag = Arc::new(QsDagAuditSink::new("futures-measurement")?);
    let audit: Arc<dyn AuditSink> = Arc::new(CompositeAuditSink::new(vec![audit_tuple, audit_dag]));

    let cfg = MetricsConfig::default();
    let engine = MetricsEngine::new(cfg, audit);

    let adapter: Arc<dyn ExecutionAdapter> = match args.venue.to_ascii_lowercase().as_str() {
        "trading-station" | "tradingstation" | "ts" => Arc::new(TradingStationAdapter {
            venue: venue.clone(),
        }),
        _ => Arc::new(AutheoAdapter::new(venue.clone())),
    };

    let mut base_mid = 4800.0;

    for i in 0..args.iters {
        p(&format!("iter {i}/{}, start", args.iters));
        // Market snapshot before decision.
        base_mid *= 1.0 + rand::thread_rng().gen_range(-0.0002..=0.0002);
        let book = synth_book(base_mid);
        let now = SystemTime::now();
        p("observe market data");
        engine.observe(&Event::MarketData(MarketData {
            ts: now,
            symbol: symbol.clone(),
            venue: venue.clone(),
            book: book.clone(),
        }))?;

        let intent_id = ClientIntentId(format!("intent-{i}-{}", rand::thread_rng().gen::<u64>()));
        let strategy_id = StrategyId("demo".into());

        let intent = StrategyIntent {
            ts: now,
            strategy_id: strategy_id.clone(),
            intent_id: intent_id.clone(),
            params: OrderParams {
                symbol: symbol.clone(),
                venue: venue.clone(),
                side,
                order_type,
                tif: TimeInForce::Ioc,
                qty_contracts: args.qty,
                limit_price: if order_type == OrderType::Limit {
                    Some(base_mid)
                } else {
                    None
                },
            },
            reference_price: book.mid(),
            book: Some(book),
        };
        p("observe intent");
        engine.observe(&Event::StrategyIntent(intent))?;

        p("place_order");
        let (_order_id, events) = adapter
            .place_order(
                intent_id.clone(),
                OrderParams {
                    symbol: symbol.clone(),
                    venue: venue.clone(),
                    side,
                    order_type,
                    tif: TimeInForce::Ioc,
                    qty_contracts: args.qty,
                    limit_price: if order_type == OrderType::Limit {
                        Some(base_mid)
                    } else {
                        None
                    },
                },
            )
            .await?;
        p(&format!("place_order returned {} events", events.len()));

        // Feed execution events.
        let mut last_ts = now;
        let mut fill_ts: Option<SystemTime> = None;
        for e in events {
            last_ts = e.ts();
            if let Event::OrderFill(of) = &e {
                if of.is_final {
                    fill_ts = Some(of.ts);
                }
            }
            p("observe execution event");
            engine.observe(&e)?;
        }

        // After a fill, publish books at the horizon times and tick.
        if let Some(ft) = fill_ts {
            p("post-fill horizons");
            // Simulate a slight adverse move after trade.
            let adverse = base_mid * (1.0 + (side.sign() * 0.00015));
            for horizon in [100u64, 1_000u64, 5_000u64] {
                let ts = ft + Duration::from_millis(horizon);
                p(&format!("observe market data horizon {horizon}ms"));
                engine.observe(&Event::MarketData(MarketData {
                    ts,
                    symbol: symbol.clone(),
                    venue: venue.clone(),
                    book: synth_book(adverse),
                }))?;
                p(&format!("tick horizon {horizon}ms"));
                engine.tick(ts)?;
            }
        } else {
            p("no fill; tick for timeouts");
            // Still tick so timeouts/cancels can be processed.
            engine.tick(last_ts + Duration::from_millis(3_000))?;
        }
        p(&format!("iter {i} done"));
    }

    let snapshot = engine.snapshot_kv();
    println!("{}", serde_json::to_string_pretty(&snapshot)?);
    Ok(())
}
