let ws = null;

const $ = (id) => document.getElementById(id);
const logEl = $("log");
const statusEl = $("connStatus");

const LS_STREAMER_URL = "tt_streamer_url";
const LS_STREAMER_TOKEN = "tt_streamer_token";

function setStatus(ok, text) {
  statusEl.textContent = text;
  statusEl.classList.toggle("ok", !!ok);
  statusEl.classList.toggle("bad", ok === false);
}

function log(line, obj) {
  const ts = new Date().toISOString();
  const msg = obj !== undefined ? `${line} ${JSON.stringify(obj)}` : line;
  const div = document.createElement("div");
  div.textContent = `[${ts}] ${msg}`;
  logEl.prepend(div);
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

    // Server wraps some messages.
    if (msg?.type === "stream" && msg.payload !== undefined) {
      log("<- stream", msg.payload);
      if (typeof msg.payload === "object") updateTopOfBook(msg.payload);
      return;
    }

    log("<-", msg);
    if (typeof msg === "object") updateTopOfBook(msg);
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

// -------- PQCNet WASM demo --------

function hex(u8) {
  return Array.from(u8)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

async function loadPqcWasm() {
  const res = await fetch("/wasm/autheo_pqc_wasm.wasm", { cache: "no-store" });
  if (!res.ok) throw new Error(`WASM fetch failed: ${res.status}`);

  const bytes = await res.arrayBuffer();
  const { instance } = await WebAssembly.instantiate(bytes, {});
  const exp = instance.exports;

  if (!exp.memory || !exp.pqc_alloc || !exp.pqc_free || !exp.pqc_handshake) {
    throw new Error("WASM missing expected exports (memory/pqc_alloc/pqc_free/pqc_handshake)");
  }

  return exp;
}

async function runHandshake() {
  const out = $("handshakeOut");
  out.textContent = "loading wasm...";

  try {
    const exp = await loadPqcWasm();
    const enc = new TextEncoder();
    const reqBytes = enc.encode($("handshakeReq").value);

    const reqPtr = exp.pqc_alloc(reqBytes.length) >>> 0;
    const respLen = 4096;
    const respPtr = exp.pqc_alloc(respLen) >>> 0;

    const mem = new Uint8Array(exp.memory.buffer);
    mem.set(reqBytes, reqPtr);

    const rc = exp.pqc_handshake(reqPtr, reqBytes.length, respPtr, respLen);

    if (rc < 0) {
      out.textContent = `pqc_handshake error: ${rc}`;
    } else {
      const resp = mem.slice(respPtr, respPtr + rc);
      out.textContent = `bytes=${rc}\nhex=${hex(resp).slice(0, 800)}${rc > 400 ? "…" : ""}`;
    }

    exp.pqc_free(reqPtr, reqBytes.length);
    exp.pqc_free(respPtr, respLen);
  } catch (e) {
    out.textContent = `handshake failed: ${String(e)}`;
  }
}

$("runHandshake").onclick = runHandshake;

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
    renderRescuePlot(cur, data.candidates || []);
  } catch (e) {
    showRescueErr(String(e));
  } finally {
    $("rescueRun").disabled = false;
  }
}

function clearRescue() {
  clearRescueErr();
  rescueRows = [];
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

