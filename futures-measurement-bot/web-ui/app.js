let ws = null;

const $ = (id) => document.getElementById(id);
const logEl = $("log");
const statusEl = $("connStatus");

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

function connect() {
  if (ws) return;
  const proto = location.protocol === "https:" ? "wss" : "ws";
  ws = new WebSocket(`${proto}://${location.host}/ws`);

  ws.onopen = () => {
    setStatus(true, "connected");
    setButtons(true);
    log("ws open");
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
