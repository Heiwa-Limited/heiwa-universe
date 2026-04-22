declare global {
  interface Window {
    HEIWA_STATUS_ENDPOINTS?: string[];
    HEIWA_STATUS_STREAMS?: string[];
  }
}

interface ProbeResult {
  url: string;
  ok: boolean;
  status: number | null;
  durationMs: number;
  payload?: unknown;
  error?: string;
}

interface WebSocketResult {
  url: string;
  ok: boolean;
  status: unknown;
  durationMs: number;
  payload?: unknown;
  error?: string;
}

const DEFAULT_HTTP_ENDPOINTS: string[] = [
  "https://api.heiwa.ltd/health",
  "https://api.heiwa.ltd/status",
];

const DEFAULT_WS_ENDPOINTS: string[] = [
  "wss://api.heiwa.ltd/status/ws",
  "wss://api.heiwa.ltd/events",
];

let activeSocket: WebSocket | null = null;

function setTransportMode(mode: string): void {
  const el = document.getElementById("transport-mode");
  if (el) el.textContent = mode;
}

function stampUpdated(): void {
  const el = document.getElementById("last-updated");
  if (el) el.textContent = new Date().toLocaleTimeString();
}

function getConfiguredEndpoints(): string[] {
  const params = new URLSearchParams(window.location.search);
  const queryEndpoints = params.getAll("endpoint").map((v) => v.trim()).filter(Boolean);
  if (queryEndpoints.length) return queryEndpoints;
  if (Array.isArray(window.HEIWA_STATUS_ENDPOINTS) && window.HEIWA_STATUS_ENDPOINTS.length) {
    return window.HEIWA_STATUS_ENDPOINTS;
  }
  return DEFAULT_HTTP_ENDPOINTS;
}

function getConfiguredWebSockets(): string[] {
  const params = new URLSearchParams(window.location.search);
  const querySockets = params.getAll("ws").map((v) => v.trim()).filter(Boolean);
  if (querySockets.length) return querySockets;
  if (Array.isArray(window.HEIWA_STATUS_STREAMS) && window.HEIWA_STATUS_STREAMS.length) {
    return window.HEIWA_STATUS_STREAMS;
  }
  return DEFAULT_WS_ENDPOINTS;
}

async function probe(url: string): Promise<ProbeResult> {
  const started = performance.now();
  try {
    const res = await fetch(url, { method: "GET", cache: "no-store" });
    const elapsed = Math.round(performance.now() - started);
    let payload: unknown = null;
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

function cardStatus(result: ProbeResult | WebSocketResult): { label: string; cls: string } {
  if (result.ok) return { label: "healthy", cls: "ok" };
  if (result.status && (result.status as number) < 500) return { label: "warning", cls: "warn" };
  return { label: "unhealthy", cls: "fail" };
}

function renderSummary(results: (ProbeResult | WebSocketResult)[]): void {
  const healthy = results.filter((r) => r.ok).length;
  const total = results.length;
  const warns = total - healthy;
  const allHealthy = healthy === total && total > 0;

  (document.getElementById("healthy-count") as HTMLElement).textContent = String(healthy);
  (document.getElementById("warn-count") as HTMLElement).textContent = String(warns);
  (document.getElementById("total-count") as HTMLElement).textContent = String(total);

  const headline = document.querySelector("#status-summary h2") as HTMLElement;
  const text = document.getElementById("status-summary-text") as HTMLElement;
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

function renderResults(results: (ProbeResult | WebSocketResult)[]): void {
  const list = document.getElementById("status-list") as HTMLElement;
  list.innerHTML = "";
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

    card.innerHTML = `
      <div class="status-card-head">
        <h2 class="mono">${result.url}</h2>
        <span class="status-badge ${state.cls}">${state.label}</span>
      </div>
      <p class="muted">HTTP ${result.status ?? "ERR"} · ${result.durationMs}ms</p>
      <pre class="status-payload">${prettyPayload}</pre>
    `;
    list.appendChild(card);
  });
}

function normalizeStreamPayload(url: string, payload: any): WebSocketResult {
  const status = payload?.status_code ?? payload?.status ?? 200;
  const explicitOk: unknown = payload?.ok;
  const explicitHealthy: unknown = payload?.healthy;
  let ok: boolean;

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

async function refreshStatus(): Promise<void> {
  const button = document.getElementById("refresh-status") as HTMLButtonElement;
  button.disabled = true;
  button.textContent = "Refreshing\u2026";
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

function connectStream(url: string): Promise<WebSocket> {
  return new Promise((resolve, reject) => {
    const socket = new WebSocket(url);
    const timeout = window.setTimeout(() => {
      socket.close();
      reject(new Error("timeout"));
    }, 4000);

    socket.addEventListener("open", () => {
      setTransportMode("websocket-live");
    });

    socket.addEventListener("message", (event: MessageEvent) => {
      window.clearTimeout(timeout);
      let payload: any;
      try {
        payload = JSON.parse(event.data as string);
      } catch {
        payload = { note: "non-json websocket payload", raw: event.data };
      }
      const results: WebSocketResult[] = Array.isArray(payload?.results)
        ? payload.results.map((item: any, index: number) =>
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

    socket.addEventListener("close", (event: CloseEvent) => {
      if (!event.wasClean) {
        setTransportMode("http-fallback");
      }
    });
  });
}

async function connectStatusStream(): Promise<WebSocket | null> {
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

export {};
