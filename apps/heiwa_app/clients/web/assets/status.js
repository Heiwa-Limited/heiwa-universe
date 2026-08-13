const DEFAULT_HTTP_ENDPOINTS = [
  "https://api.heiwa.ltd/health",
  "https://api.heiwa.ltd/status",
];

const DEFAULT_WS_ENDPOINTS = [
  "wss://api.heiwa.ltd/status/ws",
  "wss://api.heiwa.ltd/events",
];

let activeSocket = null;

function setTransportMode(mode) {
  const el = document.getElementById("transport-mode");
  if (el) el.textContent = mode;
}

function stampUpdated() {
  const el = document.getElementById("last-updated");
  if (el) el.textContent = new Date().toLocaleTimeString();
}

function allowedPublicEndpoint(value, protocol) {
  try {
    const url = new URL(value);
    return url.protocol === protocol && url.hostname === "api.heiwa.ltd" && url.port === "";
  } catch {
    return false;
  }
}

function configuredValues(queryKey, injected, defaults, protocol) {
  const params = new URLSearchParams(window.location.search);
  const queryValues = params.getAll(queryKey).map((value) => value.trim()).filter(Boolean);
  const injectedValues = Array.isArray(injected)
    ? injected.filter((value) => typeof value === "string")
    : [];
  const candidates = queryValues.length ? queryValues : injectedValues;
  const allowed = candidates.filter((value) => allowedPublicEndpoint(value, protocol));
  return allowed.length ? allowed : defaults;
}

function getConfiguredEndpoints() {
  return configuredValues(
    "endpoint",
    window.HEIWA_STATUS_ENDPOINTS,
    DEFAULT_HTTP_ENDPOINTS,
    "https:"
  );
}

function getConfiguredWebSockets() {
  return configuredValues(
    "ws",
    window.HEIWA_STATUS_STREAMS,
    DEFAULT_WS_ENDPOINTS,
    "wss:"
  );
}

async function probe(url) {
  const started = performance.now();
  try {
    const res = await fetch(url, { method: "GET", cache: "no-store" });
    const elapsed = Math.round(performance.now() - started);
    let payload = null;
    try {
      payload = await res.json();
    } catch {
      payload = { note: "Non-JSON response" };
    }
    return {
      url,
      ok: res.ok,
      status: res.status,
      durationMs: elapsed,
      payload,
    };
  } catch (error) {
    return {
      url,
      ok: false,
      status: null,
      durationMs: Math.round(performance.now() - started),
      error: String(error),
    };
  }
}

function cardStatus(result) {
  if (result.ok) return { label: "healthy", cls: "ok" };
  if (result.status && result.status < 500) return { label: "warning", cls: "warn" };
  return { label: "unhealthy", cls: "fail" };
}

function renderSummary(results) {
  const healthy = results.filter((r) => r.ok).length;
  const total = results.length;
  const warns = total - healthy;
  const allHealthy = healthy === total && total > 0;

  document.getElementById("healthy-count").textContent = String(healthy);
  document.getElementById("warn-count").textContent = String(warns);
  document.getElementById("total-count").textContent = String(total);

  const headline = document.querySelector("#status-summary h2");
  const text = document.getElementById("status-summary-text");
  if (allHealthy) {
    headline.textContent = "Platform checks healthy";
    text.textContent = "All configured endpoints returned success responses.";
  } else if (healthy > 0) {
    headline.textContent = "Partial health";
    text.textContent = "Some endpoints are healthy; review warnings below.";
  } else {
    headline.textContent = "Health checks need attention";
    text.textContent = "No configured endpoints returned a healthy response.";
  }
}

function renderResults(results) {
  const list = document.getElementById("status-list");
  list.replaceChildren();
  stampUpdated();

  results.forEach((result) => {
    const state = cardStatus(result);
    const card = document.createElement("article");
    card.className = "panel";

    const prettyPayload = JSON.stringify(
      result.error ? { error: result.error } : result.payload,
      null,
      2
    );

    const head = document.createElement("div");
    head.className = "status-card-head";
    const title = document.createElement("h2");
    title.className = "mono";
    title.textContent = result.url;
    const badge = document.createElement("span");
    badge.className = `status-badge ${state.cls}`;
    badge.textContent = state.label;
    head.append(title, badge);

    const timing = document.createElement("p");
    timing.className = "muted";
    timing.textContent = `HTTP ${result.status ?? "ERR"} · ${result.durationMs}ms`;
    const payload = document.createElement("pre");
    payload.className = "status-payload";
    payload.textContent = prettyPayload ?? "";
    card.append(head, timing, payload);
    list.appendChild(card);
  });
}

function normalizeStreamPayload(url, payload) {
  const status = payload?.status_code ?? payload?.status ?? 200;
  const explicitOk = payload?.ok;
  const explicitHealthy = payload?.healthy;
  let ok;

  if (typeof explicitOk === "boolean") {
    ok = explicitOk;
  } else if (typeof explicitHealthy === "boolean") {
    ok = explicitHealthy;
  } else if (typeof payload?.status === "string") {
    ok = payload.status.toLowerCase() === "ok";
  } else {
    ok = typeof status === "number" ? status >= 200 && status < 400 : true;
  }

  return {
    url,
    ok,
    status,
    durationMs: payload?.duration_ms ?? 0,
    payload,
  };
}

async function refreshStatus() {
  const button = document.getElementById("refresh-status");
  button.disabled = true;
  button.textContent = "Refreshing…";
  setTransportMode("http-fallback");
  try {
    const endpoints = getConfiguredEndpoints();
    const results = await Promise.all(endpoints.map(probe));
    renderSummary(results);
    renderResults(results);
  } finally {
    button.disabled = false;
    button.textContent = "Refresh";
  }
}

function connectStream(url) {
  return new Promise((resolve, reject) => {
    const socket = new WebSocket(url);
    const timeout = window.setTimeout(() => {
      socket.close();
      reject(new Error("timeout"));
    }, 4000);

    socket.addEventListener("open", () => {
      setTransportMode("websocket-live");
    });

    socket.addEventListener("message", (event) => {
      window.clearTimeout(timeout);
      let payload;
      try {
        payload = JSON.parse(event.data);
      } catch {
        payload = { note: "non-json websocket payload", raw: event.data };
      }
      const results = Array.isArray(payload?.results)
        ? payload.results.map((item, index) =>
            normalizeStreamPayload(item?.url || `${url}#${index + 1}`, item)
          )
        : [normalizeStreamPayload(url, payload)];
      renderSummary(results);
      renderResults(results);
      resolve(socket);
    });

    socket.addEventListener("error", () => {
      window.clearTimeout(timeout);
      reject(new Error("websocket error"));
    });

    socket.addEventListener("close", (event) => {
      if (!event.wasClean) {
        setTransportMode("http-fallback");
      }
    });
  });
}

async function connectStatusStream() {
  if (activeSocket) {
    try {
      activeSocket.close();
    } catch {
      // Ignore close errors from a stale socket.
    }
    activeSocket = null;
  }

  const candidates = getConfiguredWebSockets();
  for (const url of candidates) {
    try {
      activeSocket = await connectStream(url);
      return activeSocket;
    } catch {
      // Try the next candidate.
    }
  }
  await refreshStatus();
  return null;
}

document.addEventListener("DOMContentLoaded", () => {
  document.getElementById("refresh-status")?.addEventListener("click", async () => {
    await refreshStatus();
    void connectStatusStream();
  });
  void connectStatusStream();
});
