use anyhow::{anyhow, Context};
use axum::{
    extract::{ws::Message, ws::WebSocket, ws::WebSocketUpgrade, State},
    http::{HeaderValue, StatusCode, Uri},
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use futures_util::{sink::SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::sync::broadcast;
use tower_http::{
    services::ServeDir,
    set_header::SetResponseHeaderLayer,
    trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer},
};

#[derive(Clone)]
struct AppState {
    web_root: PathBuf,
    wasm_paths: WasmPaths,
    streamer: StreamerConfig,
    // Fanout for sim-mode (and potentially cached latest state later).
    sim_tx: broadcast::Sender<serde_json::Value>,
}

#[derive(Clone, Debug)]
struct WasmPaths {
    // If present, we try to serve a compiled Autheo PQC WASM from here.
    // Otherwise we fall back to web_root/wasm/autheo_pqc_wasm.wasm.
    preferred_autheo_pqc_wasm: PathBuf,
}

#[derive(Clone, Debug)]
struct StreamerConfig {
    mode: StreamMode,
    tasty: TastytradeConfig,
}

#[derive(Clone, Debug)]
enum StreamMode {
    Sim,
    TastytradeDxLink,
}

#[derive(Clone, Debug, Default)]
struct TastytradeConfig {
    api_base: String,
    username: Option<String>,
    password: Option<String>,
    streamer_url: Option<String>,
    streamer_token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ClientCmd {
    #[serde(rename = "subscribe")]
    Subscribe { symbols: Vec<String>, feed: String },
    #[serde(rename = "unsubscribe")]
    Unsubscribe { symbols: Vec<String>, feed: String },
    #[serde(rename = "raw")]
    Raw { payload: serde_json::Value },
}

#[derive(Debug, Serialize)]
struct ServerMsg {
    #[serde(rename = "type")]
    ty: &'static str,
    payload: serde_json::Value,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let web_root = PathBuf::from(env_or("WEB_UI_ROOT", "web-ui"));

    let wasm_paths = WasmPaths {
        preferred_autheo_pqc_wasm: PathBuf::from(
            env_or(
                "AUTHEO_PQC_WASM_PATH",
                "../pqcnet-contracts/target/wasm32-unknown-unknown/release/autheo_pqc_wasm.wasm",
            ),
        ),
    };

    let (sim_tx, _sim_rx) = broadcast::channel(1024);

    let streamer = StreamerConfig {
        mode: if std::env::var_os("STREAM_SIM").is_some() {
            StreamMode::Sim
        } else {
            StreamMode::TastytradeDxLink
        },
        tasty: TastytradeConfig {
            api_base: env_or("TASTYTRADE_API_BASE", "https://api.tastytrade.com"),
            username: std::env::var("TASTYTRADE_USERNAME").ok(),
            password: std::env::var("TASTYTRADE_PASSWORD").ok(),
            streamer_url: std::env::var("TASTYTRADE_STREAMER_URL").ok(),
            streamer_token: std::env::var("TASTYTRADE_STREAMER_TOKEN").ok(),
        },
    };

    if matches!(streamer.mode, StreamMode::Sim) {
        spawn_sim(sim_tx.clone());
    }

    let state = AppState {
        web_root: web_root.clone(),
        wasm_paths,
        streamer,
        sim_tx,
    };

    let static_svc = ServeDir::new(web_root.clone());

    let app = Router::new()
        .route("/", get(index))
        .route("/ws", get(ws_handler))
        .route("/wasm/autheo_pqc_wasm.wasm", get(serve_autheo_pqc_wasm))
        .nest_service(
            "/",
            static_svc.layer(SetResponseHeaderLayer::overriding(
                axum::http::header::CACHE_CONTROL,
                HeaderValue::from_static("no-store"),
            )),
        )
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().include_headers(false))
                .on_response(DefaultOnResponse::new().include_headers(false)),
        )
        .with_state(Arc::new(state));

    let bind: SocketAddr = env_or("WEB_UI_BIND", "0.0.0.0:8080")
        .parse()
        .context("invalid WEB_UI_BIND")?;

    tracing::info!(?bind, "web ui server starting");

    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

async fn index(State(state): State<Arc<AppState>>, uri: Uri) -> impl IntoResponse {
    // Ensure / returns the web-ui/index.html even if static service nesting changes.
    let p = state.web_root.join("index.html");
    match tokio::fs::read_to_string(&p).await {
        Ok(s) => Html(s).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, format!("missing {}: {e}", uri)).into_response(),
    }
}

async fn serve_autheo_pqc_wasm(State(state): State<Arc<AppState>>) -> Response {
    // 1) Prefer a built artifact path (repo-local default).
    // 2) Fall back to web-ui/wasm/autheo_pqc_wasm.wasm if user copied it there.

    let candidates = [
        state.wasm_paths.preferred_autheo_pqc_wasm.clone(),
        state.web_root.join("wasm").join("autheo_pqc_wasm.wasm"),
    ];

    for p in candidates {
        if let Ok(bytes) = tokio::fs::read(&p).await {
            let mut resp = bytes.into_response();
            resp.headers_mut().insert(
                axum::http::header::CONTENT_TYPE,
                HeaderValue::from_static("application/wasm"),
            );
            return resp;
        }
    }

    (
        StatusCode::NOT_FOUND,
        "autheo_pqc_wasm.wasm not found; build pqcnet-contracts/autheo-pqc-wasm for wasm32 and/or copy into futures-measurement-bot/web-ui/wasm/",
    )
        .into_response()
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        if let Err(e) = handle_client_ws(state, socket).await {
            tracing::warn!(error = %e, "ws session ended with error");
        }
    })
}

async fn handle_client_ws(state: Arc<AppState>, socket: WebSocket) -> anyhow::Result<()> {
    match state.streamer.mode {
        StreamMode::Sim => handle_sim_ws(state, socket).await,
        StreamMode::TastytradeDxLink => handle_tastytrade_ws(state, socket).await,
    }
}

async fn handle_sim_ws(state: Arc<AppState>, socket: WebSocket) -> anyhow::Result<()> {
    let (mut ws_tx, mut ws_rx) = socket.split();

    // Fanout from sim loop.
    let mut sim_rx = state.sim_tx.subscribe();

    // Reader: accept commands but no-op (we still log them back).
    let reader = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_rx.next().await {
            if let Message::Text(t) = msg {
                let _ = serde_json::from_str::<ClientCmd>(&t);
            }
        }
    });

    // Writer: push sim messages.
    loop {
        tokio::select! {
            _ = &mut {reader} => {
                break;
            }
            v = sim_rx.recv() => {
                let v = v?;
                let out = ServerMsg{ ty: "stream", payload: v };
                let txt = serde_json::to_string(&out)?;
                ws_tx.send(Message::Text(txt)).await?;
            }
        }
    }

    Ok(())
}

async fn handle_tastytrade_ws(state: Arc<AppState>, socket: WebSocket) -> anyhow::Result<()> {
    let (mut client_tx, mut client_rx) = socket.split();

    let (streamer_url, streamer_token) = resolve_streamer_token(&state.streamer.tasty).await?;

    let (mut upstream_ws, _resp) = tokio_tungstenite::connect_async(&streamer_url)
        .await
        .with_context(|| format!("connect streamer: {streamer_url}"))?;

    // Minimal dxLink-ish handshake. If Tastytrade changes protocol, the user can still use "raw".
    // Channel numbers are conventional.
    let setup = json!({"type":"SETUP","channel":0,"keepaliveTimeout":60,"acceptKeepaliveTimeout":60,"version":"0.1"});
    upstream_ws
        .send(tokio_tungstenite::tungstenite::Message::Text(setup.to_string()))
        .await?;
    let auth = json!({"type":"AUTH","channel":0,"token": streamer_token});
    upstream_ws
        .send(tokio_tungstenite::tungstenite::Message::Text(auth.to_string()))
        .await?;
    let channel = json!({"type":"CHANNEL_REQUEST","channel":1,"service":"FEED","parameters":{"contract":"AUTO"}});
    upstream_ws
        .send(tokio_tungstenite::tungstenite::Message::Text(channel.to_string()))
        .await?;

    // Split upstream so we can forward concurrently.
    let (mut up_tx, mut up_rx) = upstream_ws.split();

    // Forward client -> upstream.
    let mut up_tx2 = up_tx.clone();
    let client_to_up = tokio::spawn(async move {
        while let Some(Ok(msg)) = client_rx.next().await {
            if let Message::Text(t) = msg {
                let cmd = match serde_json::from_str::<ClientCmd>(&t) {
                    Ok(c) => c,
                    Err(_) => {
                        // ignore
                        continue;
                    }
                };

                match cmd {
                    ClientCmd::Subscribe { symbols, feed } => {
                        let add: Vec<serde_json::Value> = symbols
                            .into_iter()
                            .map(|s| json!({"symbol": s, "type": feed}))
                            .collect();
                        let msg = json!({"type":"FEED_SUBSCRIPTION","channel":1,"add": add});
                        let _ = up_tx2
                            .send(tokio_tungstenite::tungstenite::Message::Text(msg.to_string()))
                            .await;
                    }
                    ClientCmd::Unsubscribe { symbols, feed } => {
                        let rem: Vec<serde_json::Value> = symbols
                            .into_iter()
                            .map(|s| json!({"symbol": s, "type": feed}))
                            .collect();
                        let msg = json!({"type":"FEED_SUBSCRIPTION","channel":1,"remove": rem});
                        let _ = up_tx2
                            .send(tokio_tungstenite::tungstenite::Message::Text(msg.to_string()))
                            .await;
                    }
                    ClientCmd::Raw { payload } => {
                        let _ = up_tx2
                            .send(tokio_tungstenite::tungstenite::Message::Text(payload.to_string()))
                            .await;
                    }
                }
            }
        }
    });

    // Forward upstream -> client.
    loop {
        tokio::select! {
            _ = client_to_up => {
                break;
            }
            msg = up_rx.next() => {
                let Some(msg) = msg else { break; };
                let msg = msg?;

                let payload = match msg {
                    tokio_tungstenite::tungstenite::Message::Text(t) => {
                        serde_json::from_str::<serde_json::Value>(&t).unwrap_or_else(|_| json!({"raw": t}))
                    }
                    tokio_tungstenite::tungstenite::Message::Binary(b) => {
                        json!({"binary_len": b.len()})
                    }
                    tokio_tungstenite::tungstenite::Message::Ping(_) => {
                        continue;
                    }
                    tokio_tungstenite::tungstenite::Message::Pong(_) => {
                        continue;
                    }
                    tokio_tungstenite::tungstenite::Message::Close(_) => {
                        break;
                    }
                    _ => continue,
                };

                let out = ServerMsg{ ty: "stream", payload };
                let txt = serde_json::to_string(&out)?;
                client_tx.send(Message::Text(txt)).await?;
            }
        }
    }

    // Try to close upstream cleanly.
    let _ = up_tx
        .send(tokio_tungstenite::tungstenite::Message::Close(None))
        .await;

    Ok(())
}

async fn resolve_streamer_token(cfg: &TastytradeConfig) -> anyhow::Result<(String, String)> {
    if let (Some(url), Some(token)) = (&cfg.streamer_url, &cfg.streamer_token) {
        return Ok((url.clone(), token.clone()));
    }

    let (Some(username), Some(password)) = (&cfg.username, &cfg.password) else {
        return Err(anyhow(
            "Missing Tastytrade credentials. Provide TASTYTRADE_STREAMER_URL + TASTYTRADE_STREAMER_TOKEN, or TASTYTRADE_USERNAME + TASTYTRADE_PASSWORD.",
        ));
    };

    let client = reqwest::Client::new();

    // 1) Create session.
    let login_url = format!("{}/sessions", cfg.api_base.trim_end_matches('/'));
    let resp = client
        .post(login_url)
        .json(&json!({"login": username, "password": password, "remember-me": true}))
        .send()
        .await
        .context("POST /sessions")?;

    let body: serde_json::Value = resp.json().await.context("parse /sessions json")?;
    let token = body
        .pointer("/data/session-token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("/sessions response missing data.session-token"))?;

    // 2) Try a couple known endpoints for quote-streamer tokens.
    let endpoints = ["quote-streamer-tokens", "api-quote-tokens", "quote-streamer-tokens" აც];
    let mut last_err: Option<anyhow::Error> = None;

    for ep in endpoints {
        let url = format!("{}/{}", cfg.api_base.trim_end_matches('/'), ep);
        let resp = client
            .get(url)
            .header("Authorization", token)
            .send()
            .await;

        let resp = match resp {
            Ok(r) => r,
            Err(e) => {
                last_err = Some(e.into());
                continue;
            }
        };

        let v: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            Err(e) => {
                last_err = Some(e.into());
                continue;
            }
        };

        // Heuristic extraction.
        let streamer_token = v
            .pointer("/data/token")
            .or_else(|| v.pointer("/data/dxlink-token"))
            .or_else(|| v.pointer("/data/quote-streamer-token"))
            .and_then(|x| x.as_str());

        let streamer_url = v
            .pointer("/data/dxlink-url")
            .or_else(|| v.pointer("/data/websocket-url"))
            .or_else(|| v.pointer("/data/url"))
            .and_then(|x| x.as_str());

        if let (Some(t), Some(u)) = (streamer_token, streamer_url) {
            // Validate URL shape (helps catch accidental HTML errors).
            let _ = url::Url::parse(u).context("invalid streamer URL")?;
            return Ok((u.to_string(), t.to_string()));
        }

        last_err = Some(anyhow!("unexpected token response shape from {}", ep));
    }

    Err(last_err.unwrap_or_else(|| anyhow!("failed to resolve quote-streamer token")))
}

fn spawn_sim(tx: broadcast::Sender<serde_json::Value>) {
    tokio::spawn(async move {
        let mut t = tokio::time::interval(Duration::from_millis(250));
        let mut last = 100.0f64;
        loop {
            t.tick().await;
            last *= 1.0 + (rand::random::<f64>() - 0.5) * 0.002;
            let bid = last - 0.01;
            let ask = last + 0.01;
            let msg = json!({
                "symbol": "SIM",
                "bidPrice": (bid * 100.0).round() / 100.0,
                "askPrice": (ask * 100.0).round() / 100.0,
                "lastPrice": (last * 100.0).round() / 100.0,
                "ts": chrono::Utc::now().to_rfc3339(),
            });
            let _ = tx.send(msg);
        }
    });
}
