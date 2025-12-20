let ws = null;

const $ = (id) => document.getElementById(id);
const logEl = $("log");
const statusEl = $("connStatus");
const pqcWasmStatusEl = $("pqcWasmStatus");
const pqcAttestStatusEl = $("pqcAttestStatus");
const pqcFpServerEl = $("pqcFpServer");
const pqcFpBrowserEl = $("pqcFpBrowser");

// Open-MA detector UI
const openMaAlertEl = $("openMaAlert");
const openMaPhaseEl = $("openMaPhase");
const openMaActiveSinceEl = $("openMaActiveSince");
const openMaLastBeginEl = $("openMaLastBegin");
const openMaLastEndEl = $("openMaLastEnd");

const LS_STREAMER_URL = "tt_streamer_url";
const LS_STREAMER_TOKEN = "tt_streamer_token";

function setStatus(ok, text) {
  statusEl.textContent = text;
  statusEl.classList.toggle("ok", !!ok);
  statusEl.classList.toggle("bad", ok === false);
}

function setText(el, text) {
  if (!el) return;
  el.textContent = text;
}

function setPqcWasmStatus(ok, text) {
  setText(pqcWasmStatusEl, text);
  pqcWasmStatusEl?.classList?.toggle?.("ok", !!ok);
  pqcWasmStatusEl?.classList?.toggle?.("bad", ok === false);
}

function setPqcAttestStatus(ok, text) {
  setText(pqcAttestStatusEl, text);
  pqcAttestStatusEl?.classList?.toggle?.("ok", !!ok);
  pqcAttestStatusEl?.classList?.toggle?.("bad", ok === false);
}

function log(line, obj) {
  const ts = new Date().toISOString();
  const msg = obj !== undefined ? `${line} ${JSON.stringify(obj)}` : line;
  const div = document.createElement("div");
  div.textContent = `[${ts}] ${msg}`;
  logEl.prepend(div);
}

function setAlert(el, kind, text) {
  if (!el) return;
  el.textContent = text;
  el.classList.toggle("ok", kind === "ok");
  el.classList.toggle("bad", kind === "bad");
}

function parseSymbols() {
  return $("symbols")
    .value.split(",")
    .map((s) => s.trim())
    .filter(Boolean);
}

function setButtons(connected) {
  $("connect").disabled = connected;
  $("disconnect").disabled = !connected;
  $("subscribe").disabled = !connected;
  $("unsubscribe").disabled = !connected;
  $("sendRaw").disabled = !connected;
}

function updateTopOfBook(msg) {
  // Best-effort: handle a few common shapes.
  // If we can't parse, we just leave the widgets alone.
  try {
    const sym = msg.symbol || msg.eventSymbol || msg?.data?.symbol || msg?.data?.eventSymbol;
    const bid = msg.bidPrice ?? msg?.data?.bidPrice ?? msg?.data?.bid;
    const ask = msg.askPrice ?? msg?.data?.askPrice ?? msg?.data?.ask;
    const last = msg.lastPrice ?? msg?.data?.lastPrice ?? msg?.data?.last;

    if (sym) $("lastSym").textContent = String(sym);
    if (bid !== undefined) $("bid").textContent = String(bid);
    if (ask !== undefined) $("ask").textContent = String(ask);
    if (last !== undefined) $("last").textContent = String(last);
  } catch {
    // ignore
  }
}

function loadStreamerSettings() {
  const url = localStorage.getItem(LS_STREAMER_URL) || "";
  const token = localStorage.getItem(LS_STREAMER_TOKEN) || "";
  if ($("streamerUrl")) $("streamerUrl").value = url;
  if ($("streamerToken")) $("streamerToken").value = token;
}

function persistStreamerSettings() {
  const url = $("streamerUrl")?.value?.trim?.() ?? "";
  const token = $("streamerToken")?.value?.trim?.() ?? "";
  localStorage.setItem(LS_STREAMER_URL, url);
  localStorage.setItem(LS_STREAMER_TOKEN, token);
}

function sendStreamerConfigIfPresent() {
  const url = $("streamerUrl")?.value?.trim?.() ?? "";
  const token = $("streamerToken")?.value?.trim?.() ?? "";

  // Only send if both are present; otherwise the server will fall back to env vars.
  if (url && token) {
    send({ type: "configure_streamer", streamerUrl: url, streamerToken: token });
  } else if (url || token) {
    log("streamer config incomplete; provide both URL + Token or set server env vars");
  }
}

function connect() {
  if (ws) return;
  const proto = location.protocol === "https:" ? "wss" : "ws";
  ws = new WebSocket(`${proto}://${location.host}/ws`);

  ws.onopen = () => {
    setStatus(true, "connected");
    setButtons(true);
    log("ws open");
    persistStreamerSettings();
    sendStreamerConfigIfPresent();
    // Optional: PQC-attest this browser session to the bot.
    if ($("pqcAutoAttest")?.checked) {
      // Fire-and-forget; doesn't block UI.
      attestSession().catch(() => {});
    }
  };

  ws.onclose = (e) => {
    log("ws close", { code: e.code, reason: e.reason });
    ws = null;
    setStatus(false, "disconnected");
    setButtons(false);
  };

  ws.onerror = () => {
    setStatus(false, "error");
  };

  ws.onmessage = (evt) => {
    const raw = evt.data;
    let msg;
    try {
      msg = JSON.parse(raw);
    } catch {
      log("<-", raw);
      return;
    }

    // PQC status / acknowledgements.
    if (msg?.type === "pqc_status") {
      const fp = msg?.payload?.fingerprint || msg?.payload?.fp;
      if (fp) setText(pqcFpServerEl, String(fp));
      setPqcAttestStatus(true, "active");
      log("<- pqc_status", msg.payload);
      return;
    }

    // Server wraps some messages.
    if (msg?.type === "stream" && msg.payload !== undefined) {
      log("<- stream", msg.payload);
      if (typeof msg.payload === "object") updateTopOfBook(msg.payload);
      if (typeof msg.payload === "object") openMaIngestStream(msg.payload);
      return;
    }

    log("<-", msg);
    if (typeof msg === "object") updateTopOfBook(msg);
    if (typeof msg === "object") openMaIngestStream(msg);
  };

  setStatus(null, "connecting...");
}

function disconnect() {
  if (!ws) return;
  ws.close(1000, "user");
}

function send(obj) {
  if (!ws) return;
  ws.send(JSON.stringify(obj));
  log("->", obj);
}

$("connect").onclick = connect;
$("disconnect").onclick = disconnect;
$("subscribe").onclick = () => send({ type: "subscribe", symbols: parseSymbols(), feed: $("feed").value });
$("unsubscribe").onclick = () => send({ type: "unsubscribe", symbols: parseSymbols(), feed: $("feed").value });
$("sendRaw").onclick = () => {
  const t = $("raw").value.trim();
  if (!t) return;
  try {
    send({ type: "raw", payload: JSON.parse(t) });
  } catch (e) {
    log("raw JSON parse error", { error: String(e) });
  }
};
$("clearLog").onclick = () => (logEl.innerHTML = "");

setButtons(false);
setStatus(false, "disconnected");

loadStreamerSettings();
$("streamerUrl")?.addEventListener("change", persistStreamerSettings);
$("streamerToken")?.addEventListener("change", persistStreamerSettings);

// -------- Open-MA Trend Window Detector (live, client-side) --------

function fmtTsUTC(ms) {
  if (!Number.isFinite(ms)) return "—";
  return new Date(ms).toISOString();
}

function fN(x, digits) {
  return Number.isFinite(x) ? x.toFixed(digits) : "—";
}

function clamp(v, lo, hi) {
  return Math.max(lo, Math.min(hi, v));
}

function parseIntSafe(t, fallback) {
  const v = Number(String(t ?? "").trim());
  const n = Math.floor(v);
  return Number.isFinite(n) && n > 0 ? n : fallback;
}

function parseFloatSafe(t, fallback) {
  const v = Number(String(t ?? "").trim());
  return Number.isFinite(v) ? v : fallback;
}

function parseStreamFields(msg) {
  // Best-effort extraction across sim + various websocket payload shapes.
  const root = Array.isArray(msg) ? msg[0] || msg : msg;
  const inner = Array.isArray(root?.data) ? root.data[0] : root?.data;
  const sym = root?.symbol || root?.eventSymbol || inner?.symbol || inner?.eventSymbol || null;
  const bid =
    root?.bidPrice ??
    inner?.bidPrice ??
    inner?.bid ??
    root?.bid ??
    root?.bestBid ??
    inner?.bestBid;
  const ask =
    root?.askPrice ??
    inner?.askPrice ??
    inner?.ask ??
    root?.ask ??
    root?.bestAsk ??
    inner?.bestAsk;
  const last =
    root?.lastPrice ??
    inner?.lastPrice ??
    inner?.last ??
    root?.last ??
    root?.price ??
    inner?.price;
  const tsRaw = root?.ts || inner?.ts || root?.timestamp || inner?.timestamp || null;
  const tsMs = tsRaw ? Date.parse(tsRaw) : Date.now();
  return { sym, bid, ask, last, tsMs: Number.isFinite(tsMs) ? tsMs : Date.now() };
}

function selectPrice(fields, source) {
  const bid = fields.bid;
  const ask = fields.ask;
  const last = fields.last;
  if (source === "bid") return Number.isFinite(bid) ? Number(bid) : null;
  if (source === "ask") return Number.isFinite(ask) ? Number(ask) : null;
  if (source === "mid") {
    if (Number.isFinite(bid) && Number.isFinite(ask)) return (Number(bid) + Number(ask)) / 2;
    return null;
  }
  // default: last
  return Number.isFinite(last) ? Number(last) : null;
}

function makeRollingSma(period) {
  return {
    period,
    q: [],
    sum: 0,
    push(v) {
      if (!Number.isFinite(v)) return null;
      this.q.push(v);
      this.sum += v;
      if (this.q.length > this.period) this.sum -= this.q.shift();
      if (this.q.length < this.period) return null;
      return this.sum / this.period;
    },
    reset(period) {
      this.period = period;
      this.q = [];
      this.sum = 0;
    },
  };
}

const openMaState = (() => {
  const st = {
    paused: false,
    seq: 0,
    points: [], // {seq, tsMs, price, fSma, sSma}
    windows: [], // {dir, startSeq, endSeq|null, startTsMs, endTsMs|null, startPx, endPx|null}
    active: null, // {dir, startSeq}
    cfg: {
      fast: 10,
      slow: 20,
      slopeLb: 3,
      minGapPct: 0.005,
      minSlopePctPerBar: 0.001,
      maxPts: 600,
      source: "last",
      symbol: "SIM",
    },
    smaFast: makeRollingSma(10),
    smaSlow: makeRollingSma(20),
  };
  return st;
})();

function openMaReadCfgFromUI() {
  const sym = $("openMaSymbol")?.value?.trim?.() ?? openMaState.cfg.symbol;
  const source = $("openMaSource")?.value ?? openMaState.cfg.source;

  const fast = parseIntSafe($("openMaFast")?.value, 10);
  const slow = parseIntSafe($("openMaSlow")?.value, 20);
  const slopeLb = parseIntSafe($("openMaSlopeLb")?.value, 3);
  const minGapPct = parseFloatSafe($("openMaMinGapPct")?.value, 0.5) / 100.0;
  const minSlopePctPerBar = parseFloatSafe($("openMaMinSlopePct")?.value, 0.1) / 100.0;
  const maxPts = clamp(parseIntSafe($("openMaMaxPts")?.value, 600), 60, 5000);

  openMaState.cfg = { ...openMaState.cfg, symbol: sym, source, fast, slow, slopeLb, minGapPct, minSlopePctPerBar, maxPts };
}

function openMaSetKvs() {
  const st = openMaState;
  const active = st.active;
  if (st.paused) {
    setText(openMaPhaseEl, "paused");
  } else if (!active) {
    setText(openMaPhaseEl, "—");
  } else {
    setText(openMaPhaseEl, active.dir === "Up" ? "UP (open)" : "DOWN (open)");
  }

  if (active) {
    const p0 = st.points.find((p) => p.seq === active.startSeq);
    setText(openMaActiveSinceEl, p0 ? fmtTsUTC(p0.tsMs) : "—");
  } else {
    setText(openMaActiveSinceEl, "—");
  }

  const lastBegin = st.windows.length ? st.windows[st.windows.length - 1] : null;
  if (lastBegin) {
    setText(openMaLastBeginEl, `${lastBegin.dir} @ ${fmtTsUTC(lastBegin.startTsMs)}`);
  } else {
    setText(openMaLastBeginEl, "—");
  }

  const lastClosed = [...st.windows].reverse().find((w) => w.endSeq !== null);
  if (lastClosed) {
    setText(openMaLastEndEl, `${lastClosed.dir} @ ${fmtTsUTC(lastClosed.endTsMs)}`);
  } else {
    setText(openMaLastEndEl, "—");
  }
}

function openMaClassifyAtIdx(i) {
  // Mirror futures-measurement-bot/src/strategy/open_ma_trend.rs logic.
  const st = openMaState;
  const cfg = st.cfg;
  const lb = cfg.slopeLb;
  if (i < lb) return null;

  const pNow = st.points[i];
  const pThen = st.points[i - lb];
  const fNow = pNow?.fSma;
  const sNow = pNow?.sSma;
  const fThen = pThen?.fSma;
  const sThen = pThen?.sSma;

  if (!Number.isFinite(fNow) || !Number.isFinite(sNow) || !Number.isFinite(fThen) || !Number.isFinite(sThen)) return null;
  if (sNow === 0 || fNow === 0) return null;

  const gapPct = Math.abs(fNow - sNow) / Math.abs(sNow);
  if (!Number.isFinite(gapPct) || gapPct < cfg.minGapPct) return null;

  const fSlopePerBar = (fNow - fThen) / lb;
  const sSlopePerBar = (sNow - sThen) / lb;
  const fSlopePct = Math.abs(fSlopePerBar / Math.abs(fNow));
  const sSlopePct = Math.abs(sSlopePerBar / Math.abs(sNow));
  if (
    !Number.isFinite(fSlopePct) ||
    !Number.isFinite(sSlopePct) ||
    fSlopePct < cfg.minSlopePctPerBar ||
    sSlopePct < cfg.minSlopePctPerBar
  ) {
    return null;
  }

  if (fNow > sNow && fSlopePerBar > 0 && sSlopePerBar > 0) return "Up";
  if (fNow < sNow && fSlopePerBar < 0 && sSlopePerBar < 0) return "Down";
  return null;
}

function openMaTrim() {
  const st = openMaState;
  const keep = st.cfg.maxPts;
  if (st.points.length <= keep) return;
  const drop = st.points.length - keep;
  st.points.splice(0, drop);
  // windows are keyed by seq (global), so trimming points doesn't require reindexing.
}

function openMaPushPoint(tsMs, price) {
  const st = openMaState;
  st.seq += 1;
  const seq = st.seq;

  const fSma = st.smaFast.push(price);
  const sSma = st.smaSlow.push(price);

  st.points.push({ seq, tsMs, price, fSma, sSma });
  openMaTrim();

  const i = st.points.length - 1;
  const dirHere = openMaClassifyAtIdx(i);

  const active = st.active;
  if (!active && dirHere) {
    st.active = { dir: dirHere, startSeq: seq };
    st.windows.push({
      dir: dirHere,
      startSeq: seq,
      endSeq: null,
      startTsMs: tsMs,
      endTsMs: null,
      startPx: price,
      endPx: null,
    });
    setAlert(openMaAlertEl, dirHere === "Up" ? "ok" : "bad", `BEGIN ${dirHere} @ ${fmtTsUTC(tsMs)} px=${fN(price, 2)}`);
  } else if (active && dirHere && active.dir === dirHere) {
    // continue window
  } else if (active && dirHere && active.dir !== dirHere) {
    // close prior, begin new
    const last = st.windows[st.windows.length - 1];
    if (last && last.endSeq === null && last.dir === active.dir) {
      last.endSeq = st.points[i - 1]?.seq ?? seq;
      last.endTsMs = st.points[i - 1]?.tsMs ?? tsMs;
      last.endPx = st.points[i - 1]?.price ?? price;
    }
    st.active = { dir: dirHere, startSeq: seq };
    st.windows.push({
      dir: dirHere,
      startSeq: seq,
      endSeq: null,
      startTsMs: tsMs,
      endTsMs: null,
      startPx: price,
      endPx: null,
    });
    setAlert(
      openMaAlertEl,
      dirHere === "Up" ? "ok" : "bad",
      `SWITCH ${active.dir} → ${dirHere} @ ${fmtTsUTC(tsMs)} px=${fN(price, 2)}`
    );
  } else if (active && !dirHere) {
    // close window at i-1
    const last = st.windows[st.windows.length - 1];
    const endPt = st.points[i - 1] || st.points[i] || null;
    if (last && last.endSeq === null && endPt) {
      last.endSeq = endPt.seq;
      last.endTsMs = endPt.tsMs;
      last.endPx = endPt.price;
    }
    st.active = null;
    if (endPt) {
      setAlert(openMaAlertEl, null, `END ${last?.dir ?? ""} @ ${fmtTsUTC(endPt.tsMs)} px=${fN(endPt.price, 2)}`);
    } else {
      setAlert(openMaAlertEl, null, `END @ ${fmtTsUTC(tsMs)} px=${fN(price, 2)}`);
    }
  }
}

function openMaRenderTable() {
  const tbody = $("openMaTable")?.querySelector?.("tbody");
  if (!tbody) return;
  tbody.innerHTML = "";

  const st = openMaState;
  const rows = [];

  // Active first (if any), then closed windows newest-first.
  const activeRow = st.windows.length ? st.windows[st.windows.length - 1] : null;
  if (activeRow && activeRow.endSeq === null) rows.push(activeRow);
  for (let i = st.windows.length - 1; i >= 0; i--) {
    const w = st.windows[i];
    if (w.endSeq !== null) rows.push(w);
    if (rows.length >= 60) break;
  }

  for (const w of rows) {
    const tr = document.createElement("tr");
    const td = (txt, cls) => {
      const x = document.createElement("td");
      x.textContent = txt;
      if (cls) x.className = cls;
      return x;
    };
    const bars = w.endSeq === null ? "—" : String(Math.max(1, w.endSeq - w.startSeq + 1));
    const endTs = w.endTsMs ? fmtTsUTC(w.endTsMs) : "— (active)";
    const endPx = w.endPx != null ? fN(w.endPx, 2) : "—";
    tr.appendChild(td(w.dir, w.dir === "Up" ? "pos" : "neg"));
    tr.appendChild(td(fmtTsUTC(w.startTsMs), "mono"));
    tr.appendChild(td(endTs, "mono"));
    tr.appendChild(td(bars, "num"));
    tr.appendChild(td(fN(w.startPx, 2), "num"));
    tr.appendChild(td(endPx, "num"));
    tbody.appendChild(tr);
  }
}

function openMaRenderPlot() {
  const c = $("openMaPlot");
  if (!c) return;
  const ctx = c.getContext("2d");
  const dpr = window.devicePixelRatio || 1;
  const rect = c.getBoundingClientRect();
  const nextW = Math.max(1, Math.round(rect.width * dpr));
  const nextH = Math.max(1, Math.round(rect.height * dpr));
  if (c.width !== nextW || c.height !== nextH) {
    c.width = nextW;
    c.height = nextH;
  }
  const w = c.width;
  const h = c.height;
  ctx.clearRect(0, 0, w, h);
  ctx.fillStyle = "#070a0f";
  ctx.fillRect(0, 0, w, h);

  const st = openMaState;
  const pts = st.points;
  if (!pts.length) return;

  const pad = 24;
  const prices = pts.map((p) => p.price).filter((x) => Number.isFinite(x));
  const fVals = pts.map((p) => p.fSma).filter((x) => Number.isFinite(x));
  const sVals = pts.map((p) => p.sSma).filter((x) => Number.isFinite(x));
  const all = [...prices, ...fVals, ...sVals];
  if (all.length < 2) return;
  let yMin = Math.min(...all);
  let yMax = Math.max(...all);
  if (yMax === yMin) {
    yMax += 1;
    yMin -= 1;
  }
  const xOf = (idx) => pad + (idx / Math.max(1, pts.length - 1)) * (w - pad * 2);
  const yOf = (v) => {
    const t = (v - yMin) / (yMax - yMin);
    return h - pad - t * (h - pad * 2);
  };

  // Active window shading (start → now)
  if (st.active) {
    const startSeq = st.active.startSeq;
    const startIdx = pts.findIndex((p) => p.seq === startSeq);
    if (startIdx >= 0) {
      const x0 = xOf(startIdx);
      const x1 = xOf(pts.length - 1);
      ctx.fillStyle = st.active.dir === "Up" ? "rgba(45,212,191,0.10)" : "rgba(255,90,115,0.10)";
      ctx.fillRect(x0, pad, x1 - x0, h - pad * 2);
    }
  }

  // Axes
  ctx.strokeStyle = "#1f2a37";
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(pad, pad);
  ctx.lineTo(pad, h - pad);
  ctx.lineTo(w - pad, h - pad);
  ctx.stroke();

  // Window boundary markers (visible only)
  const visibleSeqMin = pts[0].seq;
  const visibleSeqMax = pts[pts.length - 1].seq;
  for (const win of st.windows) {
    if (win.startSeq >= visibleSeqMin && win.startSeq <= visibleSeqMax) {
      const idx = pts.findIndex((p) => p.seq === win.startSeq);
      if (idx >= 0) {
        ctx.strokeStyle = win.dir === "Up" ? "rgba(45,212,191,0.8)" : "rgba(255,90,115,0.8)";
        ctx.beginPath();
        const x = xOf(idx);
        ctx.moveTo(x, pad);
        ctx.lineTo(x, h - pad);
        ctx.stroke();
      }
    }
    if (win.endSeq != null && win.endSeq >= visibleSeqMin && win.endSeq <= visibleSeqMax) {
      const idx = pts.findIndex((p) => p.seq === win.endSeq);
      if (idx >= 0) {
        ctx.strokeStyle = "rgba(153,163,176,0.65)";
        ctx.setLineDash([4, 4]);
        ctx.beginPath();
        const x = xOf(idx);
        ctx.moveTo(x, pad);
        ctx.lineTo(x, h - pad);
        ctx.stroke();
        ctx.setLineDash([]);
      }
    }
  }

  const drawLine = (getY, color) => {
    ctx.strokeStyle = color;
    ctx.lineWidth = 2;
    ctx.beginPath();
    let started = false;
    for (let i = 0; i < pts.length; i++) {
      const yv = getY(pts[i]);
      if (!Number.isFinite(yv)) continue;
      const x = xOf(i);
      const y = yOf(yv);
      if (!started) {
        ctx.moveTo(x, y);
        started = true;
      } else {
        ctx.lineTo(x, y);
      }
    }
    if (started) ctx.stroke();
  };

  // Price + SMAs
  drawLine((p) => p.price, "rgba(230,237,243,0.9)");
  drawLine((p) => p.fSma, "rgba(79,140,255,0.95)");
  drawLine((p) => p.sSma, "rgba(45,212,191,0.95)");

  // Labels
  ctx.fillStyle = "#99a3b0";
  ctx.font = "12px ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace";
  const cfg = st.cfg;
  ctx.fillText(
    `px + SMA(${cfg.fast}) + SMA(${cfg.slow}) | gap>=${(cfg.minGapPct * 100).toFixed(2)}% | slope>=${(
      cfg.minSlopePctPerBar * 100
    ).toFixed(3)}%/bar`,
    pad,
    16
  );
}

function openMaRecomputeFromScratch() {
  openMaReadCfgFromUI();
  const st = openMaState;

  // Mirror Rust behavior: ensure fast <= slow for internal SMA calculation.
  const f0 = st.cfg.fast;
  const s0 = st.cfg.slow;
  const fast = Math.min(f0, s0);
  const slow = Math.max(f0, s0);
  st.cfg.fast = fast;
  st.cfg.slow = slow;

  st.smaFast.reset(fast);
  st.smaSlow.reset(slow);

  const oldPts = st.points.map((p) => ({ seq: p.seq, tsMs: p.tsMs, price: p.price }));
  st.points = [];
  st.windows = [];
  st.active = null;

  for (const p of oldPts) {
    const fSma = st.smaFast.push(p.price);
    const sSma = st.smaSlow.push(p.price);
    st.points.push({ ...p, fSma, sSma });
    // Replay classification / window logic by temporarily using same codepath:
    const i = st.points.length - 1;
    const dirHere = openMaClassifyAtIdx(i);
    const active = st.active;
    if (!active && dirHere) {
      st.active = { dir: dirHere, startSeq: p.seq };
      st.windows.push({
        dir: dirHere,
        startSeq: p.seq,
        endSeq: null,
        startTsMs: p.tsMs,
        endTsMs: null,
        startPx: p.price,
        endPx: null,
      });
    } else if (active && dirHere && active.dir === dirHere) {
      // continue
    } else if (active && dirHere && active.dir !== dirHere) {
      const last = st.windows[st.windows.length - 1];
      if (last && last.endSeq === null && i > 0) {
        const endPt = st.points[i - 1];
        last.endSeq = endPt.seq;
        last.endTsMs = endPt.tsMs;
        last.endPx = endPt.price;
      }
      st.active = { dir: dirHere, startSeq: p.seq };
      st.windows.push({
        dir: dirHere,
        startSeq: p.seq,
        endSeq: null,
        startTsMs: p.tsMs,
        endTsMs: null,
        startPx: p.price,
        endPx: null,
      });
    } else if (active && !dirHere) {
      const last = st.windows[st.windows.length - 1];
      if (last && last.endSeq === null && i > 0) {
        const endPt = st.points[i - 1];
        last.endSeq = endPt.seq;
        last.endTsMs = endPt.tsMs;
        last.endPx = endPt.price;
      }
      st.active = null;
    }
  }

  openMaTrim();
  openMaSetKvs();
  openMaRenderTable();
  openMaRenderPlot();
  setAlert(openMaAlertEl, null, "Recomputed windows from buffered points.");
}

function openMaClear() {
  const st = openMaState;
  st.seq = 0;
  st.points = [];
  st.windows = [];
  st.active = null;
  st.smaFast.reset(st.cfg.fast);
  st.smaSlow.reset(st.cfg.slow);
  openMaSetKvs();
  openMaRenderTable();
  openMaRenderPlot();
  setAlert(openMaAlertEl, null, "Cleared.");
}

function openMaIngestStream(payload) {
  try {
    const st = openMaState;
    if (st.paused) return;
    if (!payload || typeof payload !== "object") return;
    openMaReadCfgFromUI();

    const fields = parseStreamFields(payload);
    const watch = st.cfg.symbol?.trim?.() || "";
    if (watch && fields.sym && String(fields.sym) !== watch) return;
    if (!fields.sym && watch) return;

    const price = selectPrice(fields, st.cfg.source);
    if (!Number.isFinite(price)) return;

    // Ensure internal fast/slow period ordering (Rust swaps).
    const fast = Math.min(st.cfg.fast, st.cfg.slow);
    const slow = Math.max(st.cfg.fast, st.cfg.slow);
    if (fast !== st.smaFast.period || slow !== st.smaSlow.period) {
      // If config changed live, recompute to keep historical classification consistent.
      openMaRecomputeFromScratch();
    }

    openMaPushPoint(fields.tsMs, price);
    openMaSetKvs();
    openMaRenderTable();
    openMaRenderPlot();
  } catch {
    // ignore (best-effort UI)
  }
}

// Wire buttons + lightweight auto-recompute.
$("openMaPause")?.addEventListener("click", () => {
  openMaState.paused = !openMaState.paused;
  $("openMaPause").textContent = openMaState.paused ? "Resume" : "Pause";
  openMaSetKvs();
  setAlert(openMaAlertEl, null, openMaState.paused ? "Paused (not ingesting stream)." : "Resumed.");
});
$("openMaClear")?.addEventListener("click", openMaClear);
$("openMaRecalc")?.addEventListener("click", openMaRecomputeFromScratch);

for (const id of ["openMaSymbol", "openMaSource", "openMaFast", "openMaSlow", "openMaSlopeLb", "openMaMinGapPct", "openMaMinSlopePct", "openMaMaxPts"]) {
  $(id)?.addEventListener("change", () => openMaRecomputeFromScratch());
}
window.addEventListener("resize", () => openMaRenderPlot());

// Init Open-MA UI state
openMaReadCfgFromUI();
openMaSetKvs();
openMaRenderTable();
openMaRenderPlot();

// -------- PQCNet WASM demo --------

function hex(u8) {
  return Array.from(u8)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

async function sha256Hex(u8) {
  const digest = await crypto.subtle.digest("SHA-256", u8);
  return hex(new Uint8Array(digest));
}

let pqcExportsPromise = null;

async function loadPqcWasm() {
  const res = await fetch("/wasm/autheo_pqc_wasm.wasm", { cache: "no-store" });
  if (!res.ok) {
    if (res.status === 404) {
      throw new Error(
        "WASM missing (404). Build pqcnet-contracts/autheo-pqc-wasm for wasm32-unknown-unknown or copy into web-ui/wasm/."
      );
    }
    throw new Error(`WASM fetch failed: ${res.status}`);
  }

  const bytes = await res.arrayBuffer();
  const { instance } = await WebAssembly.instantiate(bytes, {});
  const exp = instance.exports;

  if (!exp.memory || !exp.pqc_alloc || !exp.pqc_free || !exp.pqc_handshake) {
    throw new Error("WASM missing expected exports (memory/pqc_alloc/pqc_free/pqc_handshake)");
  }

  return exp;
}

async function ensurePqcWasmLoaded() {
  if (!pqcExportsPromise) {
    setPqcWasmStatus(null, "loading…");
    pqcExportsPromise = loadPqcWasm();
  }
  try {
    const exp = await pqcExportsPromise;
    setPqcWasmStatus(true, "loaded");
    return exp;
  } catch (e) {
    pqcExportsPromise = null;
    setPqcWasmStatus(false, "missing");
    throw e;
  }
}

async function pqcHandshakeBytes(requestText) {
  const exp = await ensurePqcWasmLoaded();
  const enc = new TextEncoder();
  const reqBytes = enc.encode(requestText);

  const reqPtr = exp.pqc_alloc(reqBytes.length) >>> 0;
  const respLen = 8192;
  const respPtr = exp.pqc_alloc(respLen) >>> 0;

  const mem = new Uint8Array(exp.memory.buffer);
  mem.set(reqBytes, reqPtr);

  const rc = exp.pqc_handshake(reqPtr, reqBytes.length, respPtr, respLen);

  let resp = null;
  if (rc >= 0) resp = mem.slice(respPtr, respPtr + rc);

  exp.pqc_free(reqPtr, reqBytes.length);
  exp.pqc_free(respPtr, respLen);

  if (rc < 0) throw new Error(`pqc_handshake error: ${rc}`);
  return resp;
}

async function runHandshake() {
  const out = $("handshakeOut");
  out.textContent = "loading wasm...";

  try {
    const resp = await pqcHandshakeBytes($("handshakeReq").value);
    const fp = await sha256Hex(resp);
    setText(pqcFpBrowserEl, fp);
    out.textContent = `bytes=${resp.length}\nsha256=${fp}\nhex=${hex(resp).slice(0, 800)}${
      resp.length > 400 ? "…" : ""
    }`;
  } catch (e) {
    out.textContent = `handshake failed: ${String(e)}`;
  }
}

$("runHandshake").onclick = runHandshake;

async function attestSession() {
  setPqcAttestStatus(null, "attesting…");
  try {
    const req = $("handshakeReq")?.value ?? "client=web-demo&ts=0";
    const resp = await pqcHandshakeBytes(req);
    const fp = await sha256Hex(resp);
    setText(pqcFpBrowserEl, fp);

    if (!ws) {
      setPqcAttestStatus(false, "inactive (not connected)");
      log("pqc attestation ready (connect to send)", { sha256: fp, bytes: resp.length });
      return;
    }

    // Send full envelope as hex so the server can fingerprint it too.
    send({ type: "pqc_attest", envelopeHex: hex(resp), sha256: fp });
    // The server will ACK with a pqc_status message.
  } catch (e) {
    setPqcAttestStatus(false, "failed");
    log("pqc attestation failed", { error: String(e) });
  }
}

$("loadWasm")?.addEventListener("click", () => {
  ensurePqcWasmLoaded().catch((e) => log("wasm load failed", { error: String(e) }));
});
$("sendAttest")?.addEventListener("click", () => attestSession());

// -------- Distressed Position Rescue Scanner --------

function f2(x) {
  return Number.isFinite(x) ? x.toFixed(2) : "—";
}
function f3(x) {
  return Number.isFinite(x) ? x.toFixed(3) : "—";
}
function money(x) {
  if (!Number.isFinite(x)) return "—";
  const s = x < 0 ? "-" : "";
  const v = Math.abs(x);
  return `${s}$${v.toFixed(2)}`;
}

function parseNum(id) {
  const t = $(id)?.value?.trim?.() ?? "";
  if (!t) return null;
  const v = Number(t);
  return Number.isFinite(v) ? v : null;
}

function showRescueErr(msg) {
  const el = $("rescueErr");
  el.style.display = "block";
  el.textContent = msg;
}
function clearRescueErr() {
  const el = $("rescueErr");
  el.style.display = "none";
  el.textContent = "";
}

let rescueSort = { key: "score", dir: "desc" };
let rescueRows = [];
let rescueLastPlot = { current: null, candidates: null };

// -------- Pop-out modal helpers (click-to-expand) --------
let popoutState = null;

function addPopoutHint(labelEl) {
  if (!labelEl) return;
  const hint = document.createElement("span");
  hint.className = "popoutHint";
  hint.textContent = "(click to expand)";
  labelEl.appendChild(hint);
}

function closePopout() {
  if (!popoutState) return;
  const { overlay, node, placeholder, onClose } = popoutState;

  try {
    placeholder.replaceWith(node);
  } catch {
    // ignore
  }
  overlay.remove();
  document.body.classList.remove("modalOpen");
  popoutState = null;
  if (typeof onClose === "function") onClose();
}

function openPopout({ title, node, onOpen, onClose }) {
  if (!node) return;
  if (popoutState) closePopout();

  const placeholder = document.createElement("div");
  placeholder.className = "popoutPlaceholder";
  placeholder.textContent = "Popped out — press Esc or click outside to close and restore.";

  node.replaceWith(placeholder);

  const overlay = document.createElement("div");
  overlay.className = "popoutOverlay";
  overlay.setAttribute("role", "dialog");
  overlay.setAttribute("aria-modal", "true");

  const panel = document.createElement("div");
  panel.className = "popoutPanel";

  const header = document.createElement("div");
  header.className = "popoutHeader";

  const t = document.createElement("div");
  t.className = "popoutTitle";
  t.textContent = title || "Expanded view";

  const closeBtn = document.createElement("button");
  closeBtn.className = "popoutClose";
  closeBtn.type = "button";
  closeBtn.textContent = "Close";
  closeBtn.addEventListener("click", closePopout);

  header.appendChild(t);
  header.appendChild(closeBtn);

  const body = document.createElement("div");
  body.className = "popoutBody";
  body.appendChild(node);

  panel.appendChild(header);
  panel.appendChild(body);
  overlay.appendChild(panel);

  overlay.addEventListener("click", (e) => {
    if (e.target === overlay) closePopout();
  });

  const onKey = (e) => {
    if (e.key === "Escape") closePopout();
  };
  window.addEventListener("keydown", onKey, { capture: true });

  document.body.appendChild(overlay);
  document.body.classList.add("modalOpen");

  popoutState = {
    overlay,
    node,
    placeholder,
    onClose: () => {
      window.removeEventListener("keydown", onKey, { capture: true });
      if (typeof onClose === "function") onClose();
    },
  };

  if (typeof onOpen === "function") onOpen();
  // Best-effort focus to make ESC feel responsive.
  closeBtn.focus({ preventScroll: true });
}

function sortRescueRows() {
  const { key, dir } = rescueSort;
  const m = dir === "asc" ? 1 : -1;
  rescueRows.sort((a, b) => {
    const av = a[key];
    const bv = b[key];
    if (typeof av === "number" && typeof bv === "number") return (av - bv) * m;
    return String(av).localeCompare(String(bv)) * m;
  });
}

function renderRescueTable() {
  const tbody = $("rescueTable").querySelector("tbody");
  tbody.innerHTML = "";
  for (const r of rescueRows) {
    const tr = document.createElement("tr");
    const td = (txt, cls) => {
      const x = document.createElement("td");
      x.textContent = txt;
      if (cls) x.className = cls;
      return x;
    };

    tr.appendChild(td(f2(r.score), "num"));
    tr.appendChild(td(r.route));
    tr.appendChild(td(String(r.dte), "num"));
    tr.appendChild(td(f2(r.short), "num"));
    tr.appendChild(td(f2(r.long), "num"));
    tr.appendChild(td(f3(r.credit), "num"));
    tr.appendChild(td(f2(r.be), "num"));
    tr.appendChild(td(money(r.theta), "num"));
    tr.appendChild(td(money(r.risk), "num"));

    const beCls = r.be_d < 0 ? "num pos" : r.be_d > 0 ? "num neg" : "num";
    const thCls = r.theta_d > 0 ? "num pos" : r.theta_d < 0 ? "num neg" : "num";
    const rkCls = r.risk_d > 0 ? "num neg" : r.risk_d < 0 ? "num pos" : "num";
    tr.appendChild(td(f2(r.be_d), beCls));
    tr.appendChild(td(money(r.theta_d), thCls));
    tr.appendChild(td(money(r.risk_d), rkCls));

    tbody.appendChild(tr);
  }
}

function renderRescuePlot(current, candidates) {
  const c = $("rescuePlot");
  const ctx = c.getContext("2d");
  // Keep the canvas crisp even when CSS scales it.
  // (We set CSS width:100% / height:240px; here we match the backing store to the display size.)
  const dpr = window.devicePixelRatio || 1;
  const rect = c.getBoundingClientRect();
  const nextW = Math.max(1, Math.round(rect.width * dpr));
  const nextH = Math.max(1, Math.round(rect.height * dpr));
  if (c.width !== nextW || c.height !== nextH) {
    c.width = nextW;
    c.height = nextH;
  }
  const w = c.width;
  const h = c.height;

  ctx.clearRect(0, 0, w, h);
  ctx.fillStyle = "#070a0f";
  ctx.fillRect(0, 0, w, h);

  const pts = [
    { be: current.break_even, th: current.net_theta_per_day, kind: "cur" },
    ...candidates.slice(0, 40).map((x) => ({
      be: x.metrics.break_even,
      th: x.metrics.net_theta_per_day,
      kind: "cand",
    })),
  ].filter((p) => Number.isFinite(p.be) && Number.isFinite(p.th));

  if (pts.length < 2) return;

  const pad = 22;
  const beMin = Math.min(...pts.map((p) => p.be));
  const beMax = Math.max(...pts.map((p) => p.be));
  const thMin = Math.min(...pts.map((p) => p.th));
  const thMax = Math.max(...pts.map((p) => p.th));

  const xOf = (be) => {
    const t = beMax === beMin ? 0.5 : (be - beMin) / (beMax - beMin);
    return pad + t * (w - pad * 2);
  };
  const yOf = (th) => {
    const t = thMax === thMin ? 0.5 : (th - thMin) / (thMax - thMin);
    return h - pad - t * (h - pad * 2);
  };

  // axes
  ctx.strokeStyle = "#1f2a37";
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(pad, pad);
  ctx.lineTo(pad, h - pad);
  ctx.lineTo(w - pad, h - pad);
  ctx.stroke();

  // zero theta line (if in range)
  if (thMin < 0 && thMax > 0) {
    const y0 = yOf(0);
    ctx.strokeStyle = "#27415f";
    ctx.setLineDash([4, 4]);
    ctx.beginPath();
    ctx.moveTo(pad, y0);
    ctx.lineTo(w - pad, y0);
    ctx.stroke();
    ctx.setLineDash([]);
  }

  // points
  for (const p of pts) {
    const x = xOf(p.be);
    const y = yOf(p.th);
    ctx.beginPath();
    ctx.arc(x, y, p.kind === "cur" ? 5 : 3, 0, Math.PI * 2);
    ctx.fillStyle = p.kind === "cur" ? "#2dd4bf" : "#4f8cff";
    ctx.fill();
  }

  // labels
  ctx.fillStyle = "#99a3b0";
  ctx.font = "12px ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace";
  ctx.fillText(`BE ${f2(beMin)} → ${f2(beMax)}`, pad, 16);
  ctx.fillText(`Theta/day ${money(thMin)} → ${money(thMax)}`, pad + 220, 16);
}

function rerenderRescuePlotIfPossible() {
  if (!rescueLastPlot.current || !Array.isArray(rescueLastPlot.candidates)) return;
  renderRescuePlot(rescueLastPlot.current, rescueLastPlot.candidates);
}

async function runRescueScan() {
  clearRescueErr();

  const symbol = $("rescueSymbol").value.trim() || null;
  const underlying = parseNum("rescueUnderlying");
  const ivPct = parseNum("rescueIv");
  const dte = parseNum("rescueDte");
  const contracts = parseNum("rescueContracts");
  const shortK = parseNum("rescueShort");
  const longK = parseNum("rescueLong");
  const credit = parseNum("rescueCredit");
  const limit = parseNum("rescueLimit");

  if (
    underlying === null ||
    ivPct === null ||
    dte === null ||
    contracts === null ||
    shortK === null ||
    longK === null ||
    limit === null
  ) {
    showRescueErr("Please fill all numeric fields (credit can be blank).");
    return;
  }

  const body = {
    symbol,
    spread: {
      kind: "Put",
      short_strike: shortK,
      long_strike: longK,
      dte_days: Math.max(1, Math.floor(dte)),
      contracts: Math.max(1, Math.floor(contracts)),
    },
    inputs: {
      underlying,
      iv: ivPct / 100.0,
      r: 0.04,
      q: 0.0,
    },
    current_credit: credit === null ? null : credit,
    limit: Math.max(1, Math.floor(limit)),
  };

  $("rescueRun").disabled = true;
  try {
    const res = await fetch("/api/rescue_scan", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    });
    const data = await res.json();
    if (!res.ok) {
      showRescueErr(data?.error || `scan failed: ${res.status}`);
      return;
    }

    const cur = data.current;
    $("curCredit").textContent = f3(cur.theo_credit);
    $("curBe").textContent = f2(cur.break_even);
    $("curTheta").textContent = money(cur.net_theta_per_day);
    $("curRisk").textContent = money(cur.capital_at_risk);

    rescueRows = (data.candidates || []).map((c) => ({
      score: c.score,
      route: c.route,
      dte: c.spread.dte_days,
      short: c.spread.short_strike,
      long: c.spread.long_strike,
      credit: c.metrics.theo_credit,
      be: c.metrics.break_even,
      theta: c.metrics.net_theta_per_day,
      risk: c.metrics.capital_at_risk,
      be_d: c.deltas.break_even_change,
      theta_d: c.deltas.theta_per_day_change,
      risk_d: c.deltas.capital_at_risk_change,
    }));

    sortRescueRows();
    renderRescueTable();
    rescueLastPlot.current = cur;
    rescueLastPlot.candidates = data.candidates || [];
    renderRescuePlot(cur, rescueLastPlot.candidates);
  } catch (e) {
    showRescueErr(String(e));
  } finally {
    $("rescueRun").disabled = false;
  }
}

function clearRescue() {
  clearRescueErr();
  rescueRows = [];
  rescueLastPlot.current = null;
  rescueLastPlot.candidates = null;
  renderRescueTable();
  $("curCredit").textContent = "—";
  $("curBe").textContent = "—";
  $("curTheta").textContent = "—";
  $("curRisk").textContent = "—";
  const c = $("rescuePlot");
  c.getContext("2d").clearRect(0, 0, c.width, c.height);
}

$("rescueRun")?.addEventListener("click", runRescueScan);
$("rescueClear")?.addEventListener("click", clearRescue);

// sortable headers
for (const th of $("rescueTable")?.querySelectorAll("th") || []) {
  th.addEventListener("click", () => {
    const k = th.dataset.k;
    if (!k) return;
    if (rescueSort.key === k) {
      rescueSort.dir = rescueSort.dir === "asc" ? "desc" : "asc";
    } else {
      rescueSort.key = k;
      rescueSort.dir = "desc";
    }
    sortRescueRows();
    renderRescueTable();
  });
}

// click-to-expand: plot + candidates section
(() => {
  const plotCanvas = $("rescuePlot");
  const plotRow = plotCanvas?.closest?.(".row");
  const plotLabel = plotRow?.querySelector?.(".label");

  const table = $("rescueTable");
  const tableRow = table?.closest?.(".row");
  const tableLabel = tableRow?.querySelector?.(".label");
  const tableWrap = tableRow?.querySelector?.(".tableWrap");

  if (plotCanvas && plotRow) {
    plotCanvas.classList.add("popoutTarget");
    plotCanvas.title = "Click to expand";
    addPopoutHint(plotLabel);

    const open = () =>
      openPopout({
        title: "Theta vs Break-even (top candidates)",
        node: plotRow,
        onOpen: () => {
          rerenderRescuePlotIfPossible();
          // Re-render on resize while expanded (keeps canvas crisp).
          const onResize = () => rerenderRescuePlotIfPossible();
          window.addEventListener("resize", onResize);
          popoutState.onClose = ((orig) => () => {
            window.removeEventListener("resize", onResize);
            orig?.();
          })(popoutState.onClose);
        },
        onClose: () => rerenderRescuePlotIfPossible(),
      });

    plotCanvas.addEventListener("click", open);
    plotLabel?.addEventListener?.("click", open);
  }

  if (tableRow && tableWrap) {
    tableWrap.classList.add("popoutTarget");
    tableWrap.title = "Click label to expand";
    addPopoutHint(tableLabel);

    const open = () =>
      openPopout({
        title: "Candidates (sortable)",
        node: tableRow,
      });

    // Don't steal clicks from header sorting; open via label (and background only).
    tableLabel?.addEventListener?.("click", open);
    tableWrap.addEventListener("click", (e) => {
      if (e.target === tableWrap) open();
    });
  }
})();

// Init PQC UI state (best-effort; no network calls).
setPqcWasmStatus(null, "not loaded");
setPqcAttestStatus(null, "inactive");
setText(pqcFpServerEl, "—");
setText(pqcFpBrowserEl, "—");

