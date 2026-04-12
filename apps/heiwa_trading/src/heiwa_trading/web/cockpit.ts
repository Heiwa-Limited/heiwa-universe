// --- Auth Gate ---
// Token is stored in localStorage. If missing, show a login prompt.
const AUTH_KEY = "heiwa_trading_token";

interface AuthHeaders {
  Authorization: string;
}

interface FetchOptions extends RequestInit {
  headers?: Record<string, string>;
}

interface ChatMessage {
  role?: string;
  kind?: string;
  timestamp?: string | number;
  action?: string;
  text?: string;
}

interface Chat {
  messages?: ChatMessage[];
}

interface ServiceStatus {
  running?: boolean;
}

interface Services {
  market_supervisor?: ServiceStatus;
}

interface SourcePayload {
  status?: string;
  message?: string;
}

interface CohortWinner {
  wallet_id?: string;
}

interface LeaderboardRow {
  wallet_id: string;
  label: string;
  last_action?: string;
  equity?: any;
  trade_count?: any;
}

interface OpportunityRow {
  slug: string;
  question?: string;
  strategy?: string;
  avg_expected_value?: any;
  buy_signals?: any;
  score?: any;
}

interface LiveTop5Row {
  wallet_id: string;
  label: string;
  last_action?: string;
  promotion_score?: any;
  equity?: any;
  trade_count?: any;
}

interface CmcMoverRow {
  symbol: string;
  name?: string;
  rank?: any;
  percent_change_24h?: any;
  volume_24h?: any;
}

interface CmcSnapshot {
  top_movers_24h?: CmcMoverRow[];
}

interface Cohort {
  cohort_id?: string;
  status?: string;
  progress_percent?: any;
  winner?: CohortWinner;
  leaderboard?: LeaderboardRow[];
}

interface Supervisor {
  cohort?: Cohort;
  sources?: Record<string, SourcePayload>;
  source_snapshots?: Record<string, any>;
  top_opportunities?: OpportunityRow[];
  live_top5?: LiveTop5Row[];
}

interface PairingStatus {
  status?: string;
  message?: string;
  cockpit_origin?: string;
  paired_control_ui_devices?: number;
  pending_control_ui_requests?: number;
}

interface OpenClaw {
  profile?: string;
  gateway_port?: any;
  pairing?: PairingStatus;
}

interface Logs {
  market_supervisor?: string[];
  gateway?: string[];
}

interface BestOpportunity {
  slug: string;
}

interface Settings {
  visible_panels?: string[];
  density?: string;
}

interface StateSnapshot {
  timestamp?: string;
  best_opportunity?: BestOpportunity;
  agent_brief?: string;
  supervisor?: Supervisor;
  services?: Services;
  openclaw?: OpenClaw;
  logs?: Logs;
  settings?: Settings;
  chat?: Chat;
}

interface ActionPayload {
  snapshot?: StateSnapshot;
  dashboard_url?: string;
}

function getToken(): string {
  return localStorage.getItem(AUTH_KEY) || "";
}

function authHeaders(): AuthHeaders {
  return { Authorization: `Bearer ${getToken()}` };
}

async function authFetch(url: string, opts: FetchOptions = {}): Promise<Response> {
  const headers = { ...authHeaders(), ...(opts.headers || {}) };
  const resp = await fetch(url, { ...opts, headers });
  if (resp.status === 401 || resp.status === 403) {
    localStorage.removeItem(AUTH_KEY);
    showLoginGate();
    throw new Error("Authentication failed");
  }
  return resp;
}

function showLoginGate(): void {
  (document.querySelector(".shell") as HTMLElement).style.display = "none";
  let gate = document.getElementById("auth-gate");
  if (!gate) {
    gate = document.createElement("div");
    gate.id = "auth-gate";
    gate.innerHTML = `
      <div style="display:flex;align-items:center;justify-content:center;min-height:100vh;background:var(--bg,#0a0a0f);font-family:system-ui,sans-serif">
        <div style="background:var(--panel-bg,#12121a);border:1px solid var(--border,#1e1e2e);border-radius:12px;padding:2.5rem;max-width:380px;width:100%">
          <h1 style="color:var(--text,#e0e0e6);font-size:1.25rem;margin:0 0 0.25rem">Heiwa Trading</h1>
          <p style="color:var(--text-muted,#888);font-size:0.85rem;margin:0 0 1.5rem">Enter your operator token to continue.</p>
          <input id="auth-token-input" type="password" placeholder="Operator token"
            style="width:100%;padding:0.6rem 0.8rem;border:1px solid var(--border,#1e1e2e);border-radius:6px;background:var(--bg,#0a0a0f);color:var(--text,#e0e0e6);font-size:0.9rem;box-sizing:border-box;margin-bottom:0.75rem" />
          <button id="auth-submit-btn"
            style="width:100%;padding:0.6rem;border:none;border-radius:6px;background:var(--accent,#6366f1);color:#fff;font-size:0.9rem;cursor:pointer">
            Connect
          </button>
          <p id="auth-error" style="color:#ef4444;font-size:0.8rem;margin:0.75rem 0 0;display:none"></p>
        </div>
      </div>`;
    document.body.appendChild(gate);
    const input = document.getElementById("auth-token-input") as HTMLInputElement;
    const btn = document.getElementById("auth-submit-btn") as HTMLButtonElement;
    const err = document.getElementById("auth-error") as HTMLElement;
    async function tryLogin(): Promise<void> {
      const token = input.value.trim();
      if (!token) return;
      btn.disabled = true;
      btn.textContent = "Connecting...";
      err.style.display = "none";
      try {
        const resp = await fetch("/trading/api/auth", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ token }),
        });
        if (!resp.ok) throw new Error("Invalid token");
        localStorage.setItem(AUTH_KEY, token);
        gate!.remove();
        (document.querySelector(".shell") as HTMLElement).style.display = "";
        bootCockpit();
      } catch (e) {
        err.textContent = "Invalid token. Try again.";
        err.style.display = "block";
        btn.disabled = false;
        btn.textContent = "Connect";
      }
    }
    btn.addEventListener("click", tryLogin);
    input.addEventListener("keydown", (e: KeyboardEvent) => { if (e.key === "Enter") tryLogin(); });
    input.focus();
  }
  gate.style.display = "";
}

// Check auth on load — if no token, show gate; otherwise boot
if (!getToken()) {
  showLoginGate();
} else {
  bootCockpit();
}

function bootCockpit(): void {

const els = {
  timestamp: document.getElementById("timestamp") as HTMLElement,
  bestOpportunity: document.getElementById("best-opportunity") as HTMLElement,
  agentBrief: document.getElementById("agent-brief") as HTMLElement,
  statusChips: document.getElementById("status-chips") as HTMLElement,
  sourceHealth: document.getElementById("source-health") as HTMLElement,
  cohortSummary: document.getElementById("cohort-summary") as HTMLElement,
  cohortLeaderboard: document.getElementById("cohort-leaderboard") as HTMLElement,
  topOpportunities: document.getElementById("top-opportunities") as HTMLElement,
  liveTop5: document.getElementById("live-top5") as HTMLElement,
  coinmarketcapMovers: document.getElementById("coinmarketcap-movers") as HTMLElement,
  supervisorLog: document.getElementById("supervisor-log") as HTMLElement,
  gatewayLog: document.getElementById("gateway-log") as HTMLElement,
  openclawSummary: document.getElementById("openclaw-summary") as HTMLElement,
  actionResult: document.getElementById("action-result") as HTMLElement,
  chatMessages: document.getElementById("chat-messages") as HTMLElement,
  chatInput: document.getElementById("chat-input") as HTMLInputElement,
  sendChat: document.getElementById("send-chat") as HTMLButtonElement,
  panelCheckboxes: document.getElementById("panel-checkboxes") as HTMLElement,
  saveLayout: document.getElementById("save-layout") as HTMLButtonElement,
  resetLayout: document.getElementById("reset-layout") as HTMLButtonElement,
};

const panelNames: Record<string, string> = {
  hero: "Mission Brief",
  status: "System Status",
  controls: "Operator Controls",
  chat: "Operator Chat",
  cohort: "Active Cohort",
  leaderboard: "Cohort Leaderboard",
  opportunities: "Best Opportunities",
  live: "Live Top 5",
  movers: "CoinMarketCap Movers",
  logs: "Supervisor Log",
  openclaw: "OpenClaw Bridge",
};

const panelRefs: Record<string, HTMLElement> = Object.fromEntries(
  Array.from(document.querySelectorAll("[data-panel]")).map((element) => [(element as HTMLElement).dataset.panel!, element as HTMLElement])
);

let currentSettings: { visible_panels: string[]; density: string } = {
  visible_panels: Object.keys(panelNames),
  density: "dense",
};

function chipClass(ok: string | boolean | undefined): string {
  if (ok === "ok" || ok === true) return "chip chip-ok";
  if (ok === "warn") return "chip chip-warn";
  return "chip chip-bad";
}

function escapeHtml(value: any): string {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;");
}

function renderRows<T>(rows: T[] | null | undefined, template: (row: T) => string, fallback = "No data yet."): string {
  if (!rows || rows.length === 0) {
    return `<div class="empty">${escapeHtml(fallback)}</div>`;
  }
  return `<div class="table">${rows.map(template).join("")}</div>`;
}

function valueOrFallback(value: any, fallback = "--"): any {
  if (value === null || value === undefined || value === "") {
    return fallback;
  }
  return value;
}

function renderMetric(label: string, value: any, accent = ""): string {
  return `
    <div class="bot-stat ${accent}">
      <span class="bot-stat-label">${escapeHtml(label)}</span>
      <span class="bot-stat-value">${escapeHtml(valueOrFallback(value))}</span>
    </div>
  `;
}

function renderBotBoard<T>(rows: T[] | null | undefined, fallback: string, renderCard: (row: T, index: number) => string): string {
  if (!rows || rows.length === 0) {
    return `<div class="empty">${escapeHtml(fallback)}</div>`;
  }
  return `<div class="bot-board">${rows.map(renderCard).join("")}</div>`;
}

function renderChat(chat: Chat): void {
  const messages = chat?.messages || [];
  if (messages.length === 0) {
    els.chatMessages.innerHTML = `<div class="empty">No operator notes yet.</div>`;
    return;
  }
  els.chatMessages.innerHTML = messages
    .slice(-12)
    .map((message: ChatMessage) => {
      const role = escapeHtml(message.role || "operator");
      const kind = escapeHtml(message.kind || "note");
      const timestamp = escapeHtml(String(message.timestamp || "--"));
      const action = message.action ? ` · ${escapeHtml(message.action)}` : "";
      return `
        <article class="chat-message chat-message-role-${escapeHtml(message.role || "operator")}">
          <div class="chat-message-header">
            <span>${role} · ${kind}${action}</span>
            <span>${timestamp}</span>
          </div>
          <div class="chat-message-text">${escapeHtml(message.text || "")}</div>
        </article>
      `;
    })
    .join("");
  els.chatMessages.scrollTop = els.chatMessages.scrollHeight;
}

function renderSettings(settings: Settings): void {
  currentSettings = {
    visible_panels: settings.visible_panels || Object.keys(panelNames),
    density: settings.density || "dense",
  };
  document.body.dataset.density = currentSettings.density;
  Object.entries(panelRefs).forEach(([name, element]) => {
    if (name === "customize") {
      element.classList.remove("panel-hidden");
      return;
    }
    element.classList.toggle("panel-hidden", !currentSettings.visible_panels.includes(name));
  });
  document.querySelectorAll('input[name="density"]').forEach((input) => {
    (input as HTMLInputElement).checked = (input as HTMLInputElement).value === currentSettings.density;
  });
  els.panelCheckboxes.innerHTML = Object.entries(panelNames)
    .map(
      ([key, label]) => `
        <label>
          <input type="checkbox" data-panel-toggle="${escapeHtml(key)}" ${currentSettings.visible_panels.includes(key) ? "checked" : ""} />
          ${escapeHtml(label)}
        </label>
      `
    )
    .join("");
  els.panelCheckboxes.querySelectorAll("[data-panel-toggle]").forEach((input) => {
    input.addEventListener("change", () => {
      (input as HTMLInputElement).closest("label")!.classList.toggle("selected", (input as HTMLInputElement).checked);
    });
  });
}

function renderState(snapshot: StateSnapshot): void {
  const supervisor: Supervisor = snapshot.supervisor || {};
  const cohort: Cohort = supervisor.cohort || {};
  const services: Services = snapshot.services || {};
  const openclaw: OpenClaw = snapshot.openclaw || {};
  const sourceStatuses: Record<string, SourcePayload> = supervisor.sources || {};
  const cmcSnapshot: CmcSnapshot = ((supervisor.source_snapshots || {}).coinmarketcap || {}) as CmcSnapshot;
  const settings: Settings = snapshot.settings || {};
  const chat: Chat = snapshot.chat || {};

  els.timestamp.textContent = snapshot.timestamp || "--";
  els.bestOpportunity.textContent = snapshot.best_opportunity ? snapshot.best_opportunity.slug : "--";
  els.agentBrief.textContent = snapshot.agent_brief || "No operator brief available.";
  renderSettings(settings);
  renderChat(chat);

  const chips: string[] = [];
  chips.push(`<span class="${chipClass(services.market_supervisor?.running)}">supervisor ${services.market_supervisor?.running ? "running" : "stopped"}</span>`);
  Object.entries(sourceStatuses).forEach(([name, payload]) => {
    chips.push(`<span class="${chipClass(payload.status)}">${escapeHtml(name)} ${escapeHtml(payload.status)}</span>`);
  });
  els.statusChips.innerHTML = chips.join("");

  els.sourceHealth.innerHTML = Object.entries(sourceStatuses)
    .map(([name, payload]) => `<div class="card"><strong>${escapeHtml(name)}</strong><br>${escapeHtml(payload.message || payload.status || "unknown")}</div>`)
    .join("");

  els.cohortSummary.innerHTML = [
    `<div class="card"><strong>ID</strong><br>${escapeHtml(cohort.cohort_id || "--")}</div>`,
    `<div class="card"><strong>Status</strong><br>${escapeHtml(cohort.status || "--")}</div>`,
    `<div class="card"><strong>Progress</strong><br>${escapeHtml(cohort.progress_percent || 0)}%</div>`,
    `<div class="card"><strong>Winner</strong><br>${escapeHtml(cohort.winner?.wallet_id || "pending")}</div>`,
  ].join("");

  els.cohortLeaderboard.innerHTML = renderBotBoard(
    cohort.leaderboard || [],
    "The cohort has not opened positions yet.",
    (row: LeaderboardRow, index: number) => `
      <article class="bot-card ${index === 0 ? "bot-card-leader" : ""}">
        <div class="bot-card-top">
          <div>
            <div class="bot-card-title">${escapeHtml(row.wallet_id)}</div>
            <div class="bot-card-subtitle">${escapeHtml(row.label)}</div>
          </div>
          <span class="bot-tag">${escapeHtml(row.last_action || "HOLD")}</span>
        </div>
        <div class="bot-stats">
          ${renderMetric("Equity", row.equity, "bot-stat-accent-lime")}
          ${renderMetric("Trades", row.trade_count, "bot-stat-accent-cyan")}
          ${renderMetric("Action", row.last_action, "bot-stat-accent-violet")}
        </div>
      </article>
    `
  );

  els.topOpportunities.innerHTML = renderBotBoard(
    supervisor.top_opportunities || [],
    "No opportunities scored yet.",
    (row: OpportunityRow) => `
      <article class="bot-card">
        <div class="bot-card-top">
          <div>
            <div class="bot-card-title">${escapeHtml(row.slug)}</div>
            <div class="bot-card-subtitle">${escapeHtml(row.question || "")}</div>
          </div>
          <span class="bot-tag">${escapeHtml(row.strategy || "scan")}</span>
        </div>
        <div class="bot-stats">
          ${renderMetric("EV", row.avg_expected_value, "bot-stat-accent-lime")}
          ${renderMetric("Buy signals", row.buy_signals, "bot-stat-accent-cyan")}
          ${renderMetric("Score", row.score, "bot-stat-accent-violet")}
        </div>
      </article>
    `
  );

  els.liveTop5.innerHTML = renderBotBoard(
    supervisor.live_top5 || [],
    "No winners promoted yet.",
    (row: LiveTop5Row, index: number) => `
      <article class="bot-card ${index === 0 ? "bot-card-leader" : ""}">
        <div class="bot-card-top">
          <div>
            <div class="bot-card-title">${escapeHtml(row.wallet_id)}</div>
            <div class="bot-card-subtitle">${escapeHtml(row.label)}</div>
          </div>
          <span class="bot-tag">${escapeHtml(row.last_action || "LIVE")}</span>
        </div>
        <div class="bot-stats">
          ${renderMetric("Promotion", row.promotion_score, "bot-stat-accent-violet")}
          ${renderMetric("Equity", row.equity, "bot-stat-accent-lime")}
          ${renderMetric("Trades", row.trade_count, "bot-stat-accent-cyan")}
        </div>
      </article>
    `
  );

  els.coinmarketcapMovers.innerHTML = renderBotBoard(
    cmcSnapshot.top_movers_24h || [],
    "No CoinMarketCap movers yet.",
    (row: CmcMoverRow) => `
      <article class="bot-card">
        <div class="bot-card-top">
          <div>
            <div class="bot-card-title">${escapeHtml(row.symbol)}</div>
            <div class="bot-card-subtitle">${escapeHtml(row.name || "")}</div>
          </div>
          <span class="bot-tag">${escapeHtml(row.rank || "--")}</span>
        </div>
        <div class="bot-stats">
          ${renderMetric("24h", `${valueOrFallback(row.percent_change_24h)}%`, "bot-stat-accent-lime")}
          ${renderMetric("Rank", row.rank, "bot-stat-accent-cyan")}
          ${renderMetric("Volume", row.volume_24h, "bot-stat-accent-violet")}
        </div>
      </article>
    `
  );

  els.supervisorLog.textContent = (snapshot.logs?.market_supervisor || []).join("\n") || "No supervisor log lines yet.";
  els.gatewayLog.textContent = (snapshot.logs?.gateway || []).join("\n") || "No gateway log lines yet.";
  const pairing: PairingStatus = openclaw.pairing || {};
  els.openclawSummary.innerHTML = [
    `<div class="card"><strong>Profile</strong><br>${escapeHtml(openclaw.profile || "--")}</div>`,
    `<div class="card"><strong>Gateway Port</strong><br>${escapeHtml(openclaw.gateway_port || "--")}</div>`,
    `<div class="card"><strong>Pairing</strong><br>${escapeHtml(pairing.status || "--")}</div>`,
    `<div class="card"><strong>Allowed Origin</strong><br>${escapeHtml(pairing.cockpit_origin || "--")}</div>`,
    `<div class="card"><strong>Paired Control UIs</strong><br>${escapeHtml(pairing.paired_control_ui_devices || 0)}</div>`,
    `<div class="card"><strong>Pending Requests</strong><br>${escapeHtml(pairing.pending_control_ui_requests || 0)}</div>`,
  ].join("");
  const pairingMessage = document.getElementById("openclaw-pairing-message");
  if (pairingMessage) {
    pairingMessage.textContent = pairing.message || "OpenClaw pairing status unavailable.";
    pairingMessage.dataset.state = pairing.status || "unknown";
  }
}

async function runAction(action: string): Promise<void> {
  const popup = action === "open_openclaw_dashboard" ? window.open("about:blank", "_blank") : null;
  els.actionResult.textContent = `Running ${action}...`;
  try {
    const response = await authFetch("/trading/api/action", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ action }),
    });
    const payload: ActionPayload = await response.json();
    els.actionResult.textContent = JSON.stringify(payload, null, 2);
    if (payload.snapshot) renderState(payload.snapshot);
    if (payload.dashboard_url) {
      if (popup && !popup.closed) {
        popup.location.href = payload.dashboard_url;
        popup.focus();
      } else {
        els.actionResult.innerHTML = `OpenClaw dashboard: <a href="${escapeHtml(payload.dashboard_url)}" target="_blank" rel="noopener noreferrer">${escapeHtml(payload.dashboard_url)}</a>`;
      }
    }
  } catch (error) {
    if (popup && !popup.closed) {
      popup.close();
    }
    throw error;
  }
}

async function submitChat(text: string): Promise<void> {
  const trimmed = text.trim();
  if (!trimmed) return;
  els.actionResult.textContent = "Sending chat command...";
  const response = await authFetch("/trading/api/action", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      action: "submit_chat",
      text: trimmed,
    }),
  });
  const payload: ActionPayload = await response.json();
  els.actionResult.textContent = JSON.stringify(payload, null, 2);
  els.chatInput.value = "";
  if (payload.snapshot) renderState(payload.snapshot);
}

async function saveLayout(): Promise<void> {
  const selectedPanels = Array.from(els.panelCheckboxes.querySelectorAll("[data-panel-toggle]"))
    .filter((input) => (input as HTMLInputElement).checked)
    .map((input) => (input as HTMLElement).dataset.panelToggle!);
  const selectedDensity = (document.querySelector('input[name="density"]:checked') as HTMLInputElement | null)?.value || "dense";
  const response = await authFetch("/trading/api/action", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      action: "set_visible_panels",
      visible_panels: selectedPanels,
    }),
  });
  const panelsPayload: ActionPayload = await response.json();
  const densityResponse = await authFetch("/trading/api/action", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      action: "set_density",
      density: selectedDensity,
    }),
  });
  const densityPayload: ActionPayload = await densityResponse.json();
  els.actionResult.textContent = JSON.stringify({ panels: panelsPayload, density: densityPayload }, null, 2);
  if (panelsPayload.snapshot) renderState(panelsPayload.snapshot);
}

async function resetLayout(): Promise<void> {
  const response = await authFetch("/trading/api/settings");
  const settings: Settings = await response.json();
  await authFetch("/trading/api/action", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      action: "set_visible_panels",
      visible_panels: settings.visible_panels || Object.keys(panelNames),
    }),
  });
  await authFetch("/trading/api/action", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      action: "set_density",
      density: settings.density || "dense",
    }),
  });
  const snapshot: StateSnapshot = await authFetch("/trading/api/state").then((result) => result.json());
  renderState(snapshot);
}

document.querySelectorAll("[data-action]").forEach((button) => {
  button.addEventListener("click", () => {
    runAction((button as HTMLElement).dataset.action!).catch((error) => {
      els.actionResult.textContent = String(error);
    });
  });
});

document.querySelectorAll("[data-chat-command]").forEach((button) => {
  button.addEventListener("click", () => {
    els.chatInput.value = (button as HTMLElement).dataset.chatCommand!;
    submitChat(els.chatInput.value).catch((error) => {
      els.actionResult.textContent = String(error);
    });
  });
});

els.chatInput.addEventListener("keydown", (event: KeyboardEvent) => {
  if (event.key === "Enter" && !event.shiftKey) {
    event.preventDefault();
    submitChat(els.chatInput.value).catch((error) => {
      els.actionResult.textContent = String(error);
    });
  }
});

els.sendChat.addEventListener("click", () => {
  submitChat(els.chatInput.value).catch((error) => {
    els.actionResult.textContent = String(error);
  });
});

els.saveLayout.addEventListener("click", () => {
  saveLayout().catch((error) => {
    els.actionResult.textContent = String(error);
  });
});

els.resetLayout.addEventListener("click", () => {
  resetLayout().catch((error) => {
    els.actionResult.textContent = String(error);
  });
});

authFetch("/trading/api/state")
  .then((response) => response.json())
  .then(renderState)
  .catch((error) => {
    els.agentBrief.textContent = `Failed to load initial state: ${error}`;
  });

const events = new EventSource(`/trading/sse?token=${encodeURIComponent(getToken())}`);
events.onmessage = (event: MessageEvent) => {
  renderState(JSON.parse(event.data));
};
events.onerror = () => {
  els.agentBrief.textContent = "Live update stream disconnected. Waiting for reconnect\u2026";
};

} // end bootCockpit
