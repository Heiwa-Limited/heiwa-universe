import "./styles.css";
import {
  apiGet,
  apiPost,
  runtimeHealth,
  runtimeStatus,
  runtimeVersion,
  providersFromSnapshot,
  operatorSubscribe,
  herdPanes as loadHerdSnapshot,
  herdCommandCatalog,
  readHerdPane,
  sendHerdPane,
  runHerdPane,
  focusHerdPane,
  splitHerdPane,
  type RuntimeHealth,
  type HerdPane,
  type HerdActionResult,
  type HerdCommandSpec,
} from "./runtime";
import { OperatorClient } from "./operator/client";
import { OperatorStore } from "./operator/store";
import type { OperatorProjection, OperatorTurn } from "./operator/types";

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

type View =
  | "home"
  | "chat"
  | "windows"
  | "calendar"
  | "mail"
  | "finance"
  | "social"
  | "agents"
  | "browser"
  | "files";

type DockPreview = {
  title: string;
  lines: string[];
};

type DockItem = {
  id: View;
  label: string;
  glyph: string;
  preview: () => DockPreview;
};

type CalendarEvent = {
  id?: string;
  title?: string;
  start?: string;
  end?: string;
  date?: string;
  kind?: string;
  status?: string;
  source?: string;
  note?: string;
};

type InboxItem = {
  title?: string;
  detail?: string;
  occurred_at?: string;
  kind?: string;
  source?: string;
  [key: string]: unknown;
};

type SubApp = {
  id: View;
  title: string;
  agent: string;
  server: string;
  state: string;
  pinnedPane: string;
  skills: string[];
  tools: string[];
  personalization: string[];
};

// ═══════════════════════════════════════════════════════════════════════════
// State
// ═══════════════════════════════════════════════════════════════════════════

const app = document.querySelector<HTMLDivElement>("#app")!;
let health: RuntimeHealth | null = null;
let view: View = "home";
let calendarEvents: CalendarEvent[] = [];
let inbox: InboxItem[] = [];
let herdPanes: HerdPane[] = [];
let herdCommands: HerdCommandSpec[] = [];
let herdStatus: "checking" | "online" | "offline" = "checking";
let herdSource = "none";
let selectedPane: string | null = null;
let selectedPaneText = "";
let selectedPaneSource = "none";
let selectedPaneStatus = "";
let paneMode: "send" | "run" = "send";
let ws: WebSocket | null = null;
let calendarDate = new Date();
const OPERATOR_THREAD_ID = "default";
const operatorStore = new OperatorStore();
const operatorClient = new OperatorClient(operatorStore, {
  get: apiGet,
  post: apiPost,
  subscribe: operatorSubscribe,
  randomUUID: () => crypto.randomUUID(),
  onChange: refreshOperatorUi,
  onError: refreshOperatorUi,
});

// ═══════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════

function esc(s: string): string {
  return s.replace(/[&<>"']/g, (c) =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c] ?? c
  );
}

function timeFmt(ts?: number): string {
  if (!ts) return "";
  return new Date(ts).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

function shortDate(raw?: string): string {
  if (!raw) return "";
  const parsed = new Date(raw);
  if (Number.isNaN(parsed.getTime())) return raw;
  return parsed.toLocaleDateString([], { month: "short", day: "numeric" });
}

function connectedProviderCount(): number {
  return providersFromSnapshot(health).filter((p) => p.status === "connected").length;
}

function pendingApprovalCount(): number {
  return health?.snapshot?.data?.approvals?.pending ?? inbox.length;
}

function liveWorkerCount(): number {
  const activeTurns = operatorStore.snapshot().turns.filter((turn) =>
    turn.status === "open" || turn.status === "running" || turn.status === "cancelling"
  ).length;
  return activeTurns + herdPanes.length + (health?.snapshot?.data?.workers?.live ?? 0);
}

function nextCalendarItems(limit = 3): CalendarEvent[] {
  return calendarEvents
    .filter((event) => event.date || event.start)
    .sort((a, b) => String(a.start || a.date).localeCompare(String(b.start || b.date)))
    .slice(0, limit);
}

function emptyState(label: string): string {
  return `<span class="quiet">${esc(label)}</span>`;
}

function viewCaption(activeView: View): string {
  if (activeView === "home") return "pinned ops";
  if (activeView === "chat") return "conversation";
  return `${activeView} window`;
}

function shortenPath(raw: string): string {
  return raw.replace(/^\/Users\/[^/]+/, "~");
}

function cssToken(raw: string): string {
  return raw.toLowerCase().replace(/[^a-z0-9_-]+/g, "-");
}

function subApps(): SubApp[] {
  return [
    {
      id: "chat",
      title: "AI Ops",
      agent: "router-persona",
      server: "heiwa runtime",
      state: runtimeStatus(health),
      pinnedPane: herdPanes[0]?.pane ?? "heiwa:ops",
      skills: ["route", "summarize", "delegate"],
      tools: ["provider CLIs", "local models", "receipts"],
      personalization: ["cheapest acceptable route", "repo truth first"],
    },
    {
      id: "calendar",
      title: "Calendar",
      agent: "scheduler",
      server: "calendar sub-app",
      state: `${calendarEvents.length} items`,
      pinnedPane: "calendar:today",
      skills: ["schedule", "conflict check", "draft holds"],
      tools: ["calendar.read", "calendar.draft", "life state"],
      personalization: ["dental/sleep/training floor", "no-overlap days"],
    },
    {
      id: "mail",
      title: "Mail",
      agent: "communications",
      server: "mail sub-app",
      state: `${inbox.length} inbox rows`,
      pinnedPane: "mail:triage",
      skills: ["triage", "draft replies", "extract asks"],
      tools: ["mail.search", "mail.draft", "approval outbox"],
      personalization: ["draft first", "no external send without approval"],
    },
    {
      id: "finance",
      title: "Finance",
      agent: "cashflow",
      server: "finance sub-app",
      state: "read model pending",
      pinnedPane: "finance:ledger",
      skills: ["cashflow", "debt plan", "receipt audit"],
      tools: ["local docs", "calculators", "approval ledger"],
      personalization: ["debt-paydown phase", "no money movement"],
    },
    {
      id: "social",
      title: "Social",
      agent: "boundary",
      server: "social sub-app",
      state: "ingress pending",
      pinnedPane: "social:review",
      skills: ["context read", "draft", "boundary check"],
      tools: ["message read models", "draft outbox", "receipts"],
      personalization: ["floor before networking", "respect non-reciprocity"],
    },
    {
      id: "files",
      title: "Files",
      agent: "librarian",
      server: "files sub-app",
      state: "workspace",
      pinnedPane: "files:workspace",
      skills: ["search", "index", "source cite"],
      tools: ["repo.grep", "fs.read", "artifact log"],
      personalization: ["smallest source slice", "evidence before claim"],
    },
  ];
}

function fallbackPanes(): HerdPane[] {
  const apps = subApps();
  return [
    {
      workspace: "heiwa",
      pane: "herdr",
      agent: "multiplexer",
      state: herdStatus,
      cwd: "Heiwa.app native herd command",
      message: "live herd feed not connected",
    },
    ...apps.slice(0, 5).map((app) => ({
      workspace: app.title.toLowerCase(),
      pane: app.pinnedPane,
      agent: app.agent,
      state: "planned",
      cwd: app.server,
      message: app.skills.join(" · "),
    })),
  ];
}

function visiblePanes(): HerdPane[] {
  return herdPanes.length ? herdPanes : fallbackPanes();
}

function livePaneIds(): Set<string> {
  return new Set(herdPanes.map((pane) => pane.pane));
}

function selectedPaneRow(): HerdPane | null {
  const panes = visiblePanes();
  return panes.find((pane) => pane.pane === selectedPane) ?? panes[0] ?? null;
}

function ensureSelectedPane(): void {
  const panes = visiblePanes();
  if (!panes.length) {
    selectedPane = null;
    selectedPaneText = "";
    return;
  }
  if (!selectedPane || !panes.some((pane) => pane.pane === selectedPane)) {
    selectedPane = panes[0].pane;
  }
}

function appViewForPane(pane: HerdPane): View {
  const haystack = `${pane.workspace} ${pane.pane} ${pane.agent} ${pane.cwd}`.toLowerCase();
  if (haystack.includes("calendar")) return "calendar";
  if (haystack.includes("mail")) return "mail";
  if (haystack.includes("finance")) return "finance";
  if (haystack.includes("social")) return "social";
  if (haystack.includes("file")) return "files";
  if (haystack.includes("browser")) return "browser";
  if (haystack.includes("qwen") || haystack.includes("agent")) return "agents";
  return "windows";
}

// ═══════════════════════════════════════════════════════════════════════════
// Data loading
// ═══════════════════════════════════════════════════════════════════════════

async function loadHealth(): Promise<void> {
  health = await runtimeHealth().catch(() => ({ reachable: false, error: null, snapshot: null }));
}

async function loadCalendar(): Promise<void> {
  try {
    const [summary, today] = await Promise.all([
      apiGet<{ data?: { holds?: CalendarEvent[]; events?: CalendarEvent[] } }>("/api/v1/calendar/summary"),
      apiGet<{ data?: { calendar?: { holds?: CalendarEvent[] }; appointments?: CalendarEvent[] } }>("/api/v1/life/today"),
    ]);
    const holds = [
      ...(summary?.data?.holds ?? []),
      ...(summary?.data?.events ?? []),
      ...(today?.data?.calendar?.holds ?? []),
      ...(today?.data?.appointments ?? []),
    ];
    // Deduplicate by id
    const seen = new Set<string>();
    calendarEvents = holds.filter(e => {
      const k = e.id || `${e.title}-${e.start}-${e.date}`;
      if (seen.has(k)) return false;
      seen.add(k);
      return true;
    });
  } catch { calendarEvents = []; }
}

async function loadInbox(): Promise<void> {
  try {
    const resp = await apiGet<{ data?: { items?: InboxItem[] } }>("/api/v1/inbox");
    inbox = resp?.data?.items ?? [];
  } catch { inbox = []; }
}

async function loadHerd(): Promise<void> {
  try {
    const snapshot = await loadHerdSnapshot();
    herdPanes = snapshot.panes.map((row) => ({
      workspace: String(row.workspace ?? "heiwa"),
      pane: String(row.pane ?? "pane"),
      agent: String(row.agent ?? "-"),
      state: String(row.state ?? "unknown"),
      cwd: String(row.cwd ?? "-"),
      message: row.message ? String(row.message) : "",
    }));
    herdStatus = snapshot.status === "online" ? "online" : "offline";
    herdSource = snapshot.source || "none";
  } catch {
    herdPanes = [];
    herdStatus = "offline";
    herdSource = "none";
  }
  ensureSelectedPane();
}

async function loadHerdCommands(): Promise<void> {
  try {
    herdCommands = await herdCommandCatalog();
  } catch {
    herdCommands = [];
  }
}

function renderHerdRunControl(canControl: boolean): string {
  if (herdCommands.length) {
    return `<select id="pane-command" ${canControl ? "" : "disabled"}>
      ${herdCommands.map((command) => `<option value="${esc(command.id)}">${esc(command.label)} · ${esc(command.id)}</option>`).join("")}
    </select>`;
  }
  return `<input id="pane-command" type="text" ${canControl ? "" : "disabled"} placeholder="command id: git.status" />`;
}

async function loadSelectedPaneText(): Promise<void> {
  const pane = selectedPaneRow();
  if (!pane || !livePaneIds().has(pane.pane)) {
    selectedPaneText = "No live herdr pane selected. Start or attach panes through herdr, then refresh.";
    selectedPaneSource = "none";
    selectedPaneStatus = "offline";
    return;
  }

  selectedPaneStatus = "reading";
  const resp = await readHerdPane(pane.pane);
  selectedPaneText = resp.ok ? resp.text : `Read failed: ${resp.error || "unknown error"}`;
  selectedPaneSource = resp.source;
  selectedPaneStatus = resp.ok ? "ready" : "error";
}

async function runPaneAction(action: () => Promise<HerdActionResult>): Promise<void> {
  selectedPaneStatus = "working";
  render();
  const result = await action();
  selectedPaneStatus = result.ok ? result.message : `Error: ${result.error || result.message}`;
  await Promise.all([loadHerd(), loadSelectedPaneText()]);
  render();
}

// ═══════════════════════════════════════════════════════════════════════════
// WebSocket — legacy approval and goal refresh only
// ═══════════════════════════════════════════════════════════════════════════

function connectWS(): void {
  const proto = location.protocol === "https:" ? "wss:" : "ws:";
  const port = location.port || (location.protocol === "https:" ? "443" : "7474");
  ws = new WebSocket(`${proto}//127.0.0.1:${port}/ws/v1/events`);

  ws.onopen = () => { /* connected */ };

  ws.onmessage = (ev) => {
    try {
      const data = JSON.parse(ev.data);
      const event = data.event as string;

      if (event === "dispatch_request_appeared" || event === "dispatch_request_decided") {
        void loadInbox().then(() => render());
      }

      if (event === "goal_updated") {
        void loadHealth().then(() => render());
      }
    } catch { /* ignore malformed */ }
  };

  ws.onclose = () => {
    // Reconnect after 3s
    setTimeout(connectWS, 3000);
  };
}

// ═══════════════════════════════════════════════════════════════════════════
// Operator submission — durable stream owns all displayed rows
// ═══════════════════════════════════════════════════════════════════════════

async function sendChat(text: string): Promise<void> {
  await operatorClient.submitTurn(text);
}

// ═══════════════════════════════════════════════════════════════════════════
// Render: icon rail (left sidebar)
// ═══════════════════════════════════════════════════════════════════════════

function dockItems(): DockItem[] {
  const next = nextCalendarItems(1)[0];
  const operator = operatorStore.snapshot();
  const operatorStatus = operatorClient.state().status;
  return [
    {
      id: "home",
      label: "Home",
      glyph: "⌂",
      preview: () => ({
        title: "Pinned Ops",
        lines: [
          `${visiblePanes().length} panes`,
          `herd ${herdStatus}`,
          `source ${herdSource}`,
          `${subApps().length} sub-app agents`,
        ],
      }),
    },
    {
      id: "chat",
      label: "AI",
      glyph: "✦",
      preview: () => ({
        title: "AI Console",
        lines: [
          `${operator.messages.length} messages`,
          `${connectedProviderCount()} connected providers`,
          operatorStatus === "submitting" || operator.turns.some((turn) => turn.status === "running")
            ? "route in flight"
            : operatorStatus,
        ],
      }),
    },
    {
      id: "windows",
      label: "Windows",
      glyph: "▥",
      preview: () => ({
        title: "Multiplexer",
        lines: [
          `${herdPanes.length || 0} live herdr panes`,
          "pinned ops windows",
          "terminal + sub-app servers",
        ],
      }),
    },
    {
      id: "calendar",
      label: "Calendar",
      glyph: "◷",
      preview: () => ({
        title: "Calendar",
        lines: [
          next ? `${shortDate(next.start || next.date)} ${next.title || "Untitled"}` : "no loaded events",
          `${calendarEvents.length} local items`,
        ],
      }),
    },
    {
      id: "mail",
      label: "Mail",
      glyph: "✉",
      preview: () => ({
        title: "Mail",
        lines: [`${inbox.length} inbox items`, "draft/write gated"],
      }),
    },
    {
      id: "finance",
      label: "Finance",
      glyph: "$",
      preview: () => ({
        title: "Finance",
        lines: ["read model pending", "writes approval-gated"],
      }),
    },
    {
      id: "social",
      label: "Social",
      glyph: "@",
      preview: () => ({
        title: "Social",
        lines: ["ingress pending", "sends approval-gated"],
      }),
    },
    {
      id: "agents",
      label: "Workers",
      glyph: "◇",
      preview: () => ({
        title: "Workers",
        lines: [
          `${liveWorkerCount()} live`,
          `${operator.turns.length} known tasks`,
        ],
      }),
    },
    {
      id: "browser",
      label: "Browser",
      glyph: "⌕",
      preview: () => ({
        title: "Browser",
        lines: ["local view", "actions approval-gated"],
      }),
    },
    {
      id: "files",
      label: "Files",
      glyph: "□",
      preview: () => ({
        title: "Files",
        lines: ["workspace read model", "mutations receipt-gated"],
      }),
    },
  ];
}

function renderRail(): string {
  return `
    <nav class="icon-rail">
      <div class="rail-logo">H</div>
      ${dockItems().map((item) => {
        const preview = item.preview();
        return `<button class="rail-btn ${view === item.id ? "active" : ""}" data-view="${item.id}" aria-label="${item.label}" title="${item.label}">
          <span class="rail-glyph">${item.glyph}</span>
          <span class="dock-preview" role="tooltip">
            <strong>${esc(preview.title)}</strong>
            ${preview.lines.map((line) => `<span>${esc(line)}</span>`).join("")}
          </span>
        </button>`;
      }
      ).join("")}
      <div class="rail-spacer"></div>
      <div class="rail-status ${health?.reachable ? "online" : "offline"}" title="${runtimeStatus(health)}"></div>
    </nav>
  `;
}

// ═══════════════════════════════════════════════════════════════════════════
// Render: home / window surface
// ═══════════════════════════════════════════════════════════════════════════

function renderHome(): string {
  const panes = visiblePanes().slice(0, 8);
  const apps = subApps();

  return `
    <div class="view home-view">
      <section class="home-command">
        <div>
          <h1>Heiwa Ops</h1>
          <p>${panes.length} pinned panes · herd ${herdStatus} via ${herdSource} · ${apps.length} sub-app agents</p>
        </div>
        <button class="home-jump primary-action" data-view="windows">Open Windows</button>
      </section>

      <section class="pinned-pane-board" aria-label="Pinned terminal and ops panes">
        ${panes.map((pane, index) => `
          <button class="pinned-pane ${index === 0 ? "primary-pane" : ""} home-jump" data-view="${livePaneIds().has(pane.pane) ? "windows" : appViewForPane(pane)}" data-pane="${esc(pane.pane)}">
            <span class="pane-topline">
              <span>${esc(pane.workspace)}</span>
              <span class="state-chip ${cssToken(pane.state)}">${esc(pane.state)}</span>
            </span>
            <strong>${esc(pane.agent === "-" ? pane.pane : pane.agent)}</strong>
            <span>${esc(pane.pane)} · ${esc(shortenPath(pane.cwd))}</span>
            ${pane.message ? `<small>${esc(pane.message)}</small>` : ""}
          </button>
        `).join("")}
      </section>

      <section class="quick-grid">
        <article class="quick-widget">
          <header><span>Sub-app servers</span><strong>${apps.length}</strong></header>
          ${apps.slice(0, 4).map((app) => `
            <div class="widget-row">
              <span>${esc(app.title)}</span>
              <strong>${esc(app.server)}</strong>
            </div>
          `).join("")}
        </article>

        <article class="quick-widget">
          <header><span>Agent skills</span><strong>${apps.length}</strong></header>
          ${apps.slice(0, 4).map((app) => `
            <div class="widget-row">
              <span>${esc(app.agent)}</span>
              <strong>${esc(app.skills.join(" · "))}</strong>
            </div>
          `).join("")}
        </article>

        <article class="quick-widget">
          <header><span>Tools</span><strong>${apps.reduce((sum, app) => sum + app.tools.length, 0)}</strong></header>
          ${apps.slice(0, 4).map((app) => `
            <div class="widget-row">
              <span>${esc(app.title)}</span>
              <strong>${esc(app.tools.join(" · "))}</strong>
            </div>
          `).join("")}
        </article>

        <article class="quick-widget">
          <header><span>Personalization</span><strong>local</strong></header>
          ${apps.slice(0, 4).map((app) => `
            <div class="widget-row">
              <span>${esc(app.title)}</span>
              <strong>${esc(app.personalization.join(" · "))}</strong>
            </div>
          `).join("")}
        </article>
      </section>
    </div>
  `;
}

function renderWindows(): string {
  const panes = visiblePanes();
  const apps = subApps();
  const liveIds = livePaneIds();
  const active = selectedPaneRow();
  const canControl = Boolean(active && liveIds.has(active.pane));
  return `
    <div class="view windows-view">
      <div class="view-header compact-header">
        <h2>Windows</h2>
        <p class="muted">Pinned terminal panes, ops servers, and app agents over one runtime state.</p>
      </div>
      <div class="terminal-cockpit">
        <section class="mux-pane pane-list-panel">
          <header><span>Terminal panes</span><strong>${panes.length}</strong></header>
          <div class="pane-list">${panes.map((pane) => {
            const live = liveIds.has(pane.pane);
            const selected = active?.pane === pane.pane;
            return `
              <button class="pane-row ${selected ? "selected" : ""}" data-pane="${esc(pane.pane)}">
                <span class="pane-row-main">
                  <strong>${esc(pane.agent === "-" ? pane.workspace : pane.agent)}</strong>
                  <span>${esc(pane.pane)} · ${esc(shortenPath(pane.cwd))}</span>
                </span>
                <span class="state-chip ${cssToken(pane.state)}">${esc(live ? pane.state : "planned")}</span>
              </button>
            `;
          }).join("")}</div>
          <button class="small-action" id="refresh-herd">Refresh</button>
        </section>

        <section class="mux-pane terminal-panel">
          <header>
            <span>${active ? esc(active.pane) : "No pane"}</span>
            <strong>${esc(selectedPaneStatus || (canControl ? "ready" : "no live pane"))}</strong>
          </header>
          <pre class="terminal-output">${esc(selectedPaneText || (canControl ? "Select refresh to read pane output." : "No live pane output. Planned panes are placeholders until herdr reports real panes."))}</pre>
          <div class="pane-controls">
            <select id="pane-mode" ${canControl ? "" : "disabled"}>
              <option value="send" ${paneMode === "send" ? "selected" : ""}>send</option>
              <option value="run" ${paneMode === "run" ? "selected" : ""}>run catalog</option>
            </select>
            ${paneMode === "run" ? renderHerdRunControl(canControl) : `<input id="pane-command" type="text" ${canControl ? "" : "disabled"} placeholder="prompt text…" />`}
            <button id="pane-submit" class="small-action primary-mini" ${canControl ? "" : "disabled"}>${paneMode === "run" ? "Run" : "Send"}</button>
          </div>
          <div class="pane-actions">
            <button id="pane-read" class="small-action" ${canControl ? "" : "disabled"}>Read</button>
            <button id="pane-focus" class="small-action" ${canControl ? "" : "disabled"}>Focus</button>
            <button id="pane-split-right" class="small-action" ${canControl ? "" : "disabled"}>Split →</button>
            <button id="pane-split-down" class="small-action" ${canControl ? "" : "disabled"}>Split ↓</button>
          </div>
          <p class="pane-source">source ${esc(selectedPaneSource || herdSource)} · herd ${esc(herdStatus)} via ${esc(herdSource)}</p>
        </section>
      </div>

      <div class="multiplexer-grid compact-mux-grid">
        <section class="mux-pane">
          <header><span>Deno / herdr</span><strong>${herdStatus}</strong></header>
          <div class="mux-body">
            <div class="mux-line"><span>source</span><strong>${esc(herdSource)}</strong></div>
            <div class="mux-line"><span>desk</span><strong>packaged Deno bridge or dev http://127.0.0.1:7480</strong></div>
            <div class="mux-line"><span>cli</span><strong>herdr read/send/split/focus; run is catalog-only unless operator env opt-in</strong></div>
            <div class="mux-line"><span>mode</span><strong>native Tauri commands; terminal daemon remains authority target</strong></div>
          </div>
        </section>
        <section class="mux-pane">
          <header><span>Sub-app agents</span><strong>${apps.length}</strong></header>
          <div class="mux-body">${apps.slice(0, 5).map((app) => `
            <div class="mux-line"><span>${esc(app.title)}</span><strong>${esc(app.agent)} · ${esc(app.skills.join(" · "))}</strong></div>
          `).join("")}</div>
        </section>
        <section class="mux-pane">
          <header><span>Ops personalization</span><strong>${pendingApprovalCount()}</strong></header>
          <div class="mux-body">${apps.slice(0, 5).map((app) => `
            <div class="mux-line"><span>${esc(app.title)}</span><strong>${esc(app.personalization.join(" · "))}</strong></div>
          `).join("")}</div>
        </section>
      </div>
    </div>
  `;
}

// ═══════════════════════════════════════════════════════════════════════════
// Render: chat view (main area)
// ═══════════════════════════════════════════════════════════════════════════

function renderMessages(): string {
  const snapshot = operatorStore.snapshot();
  const transients = Object.entries(snapshot.transientByTurn).map(([turnId, body]) => ({
    id: `transient-${turnId}`,
    role: "assistant" as const,
    body,
    meta: "streaming",
    occurredAt: "",
  }));
  const messages = [
    ...snapshot.messages.map((message) => ({
      ...message,
      meta: [message.provider, message.model].filter(Boolean).join("/") || (message.receiptRef ? "receipt linked" : ""),
    })),
    ...transients,
  ];
  if (messages.length === 0) {
    const unavailable = operatorClient.state().status === "error";
    return `<div class="chat-empty">
      <div class="chat-empty-icon">✦</div>
      <p>${unavailable ? "Operator stream unavailable." : "No messages yet."}</p>
    </div>`;
  }

  return messages.map(m => {
    return `
      <article class="chat-msg ${m.role}">
        <div class="chat-msg-header">
          <span class="chat-role">${m.role}</span>
          <span class="chat-meta">${esc(m.meta || "")}</span>
          <span class="chat-time">${timeFmt(Date.parse(m.occurredAt))}</span>
        </div>
        <div class="chat-body">${esc(m.body)}</div>
      </article>
    `;
  }).join("");
}

function renderChat(): string {
  return `
    <div class="view chat-view">
      <div class="chat-messages" id="chat-messages">
        ${renderMessages()}
      </div>
    </div>
  `;
}

// ═══════════════════════════════════════════════════════════════════════════
// Render: calendar view
// ═══════════════════════════════════════════════════════════════════════════

function renderCalendar(): string {
  const today = new Date();
  const year = calendarDate.getFullYear();
  const month = calendarDate.getMonth();
  const firstDay = new Date(year, month, 1).getDay();
  const daysInMonth = new Date(year, month + 1, 0).getDate();
  const monthName = calendarDate.toLocaleDateString("en-US", { month: "long", year: "numeric" });

  // Build day cells
  let cells = "";
  // Empty cells before first day
  for (let i = 0; i < firstDay; i++) {
    cells += `<div class="cal-cell empty"></div>`;
  }
  // Day cells
  for (let d = 1; d <= daysInMonth; d++) {
    const dateStr = `${year}-${String(month + 1).padStart(2, "0")}-${String(d).padStart(2, "0")}`;
    const isToday = today.getFullYear() === year && today.getMonth() === month && today.getDate() === d;
    const dayEvents = calendarEvents.filter(e => e.date === dateStr);
    const eventDots = dayEvents.slice(0, 3).map(e =>
      `<span class="cal-dot ${e.kind || "event"}"></span>`
    ).join("");
    cells += `<div class="cal-cell ${isToday ? "today" : ""}">
      <span class="cal-day">${d}</span>
      <div class="cal-dots">${eventDots}</div>
    </div>`;
  }

  // Upcoming events list
  const upcoming = calendarEvents
    .filter(e => e.date && e.date >= `${year}-${String(month + 1).padStart(2, "0")}-01`)
    .slice(0, 10);

  return `
    <div class="view calendar-view">
      <div class="cal-header">
        <div class="cal-nav">
          <button data-cal-nav="prev" aria-label="Previous month">‹</button>
          <h2>${esc(monthName)}</h2>
          <button data-cal-nav="next" aria-label="Next month">›</button>
          <button class="cal-today" data-cal-nav="today">Today</button>
        </div>
      </div>
      <div class="cal-grid">
        <div class="cal-header-row">Sun Mon Tue Wed Thu Fri Sat</div>
        ${cells}
      </div>
      <div class="cal-upcoming">
        <h3>Upcoming</h3>
        ${upcoming.length === 0 ? '<p class="muted">No events this month.</p>' :
          upcoming.map(e => `
            <div class="cal-event-row">
              <span class="cal-event-date">${e.date}</span>
              <span class="cal-event-title">${esc(e.title || "Untitled")}</span>
              <span class="cal-event-kind">${esc(e.kind || "")}</span>
            </div>
          `).join("")
        }
      </div>
    </div>
  `;
}

// ═══════════════════════════════════════════════════════════════════════════
// Render: agents view
// ═══════════════════════════════════════════════════════════════════════════

function routeForTurn(turn: OperatorTurn, routes: Record<string, OperatorProjection>): OperatorProjection | undefined {
  return Object.values(routes)
    .filter((route) => route.turnId === turn.turnId)
    .sort((left, right) => left.occurredAt.localeCompare(right.occurredAt))
    .at(-1);
}

function routeLabel(route?: OperatorProjection): string {
  if (!route) return "auto";
  const provider = typeof route.payload.provider === "string" ? route.payload.provider : "auto";
  const model = typeof route.payload.model === "string" ? route.payload.model : "";
  return [provider, model].filter(Boolean).join("/");
}

function renderTurnRows(): string {
  const snapshot = operatorStore.snapshot();
  if (snapshot.turns.length === 0) return '<p class="muted">No operator turns yet.</p>';
  return [...snapshot.turns].reverse().map(turn => {
    const route = routeForTurn(turn, snapshot.routesByCall);
    const output = snapshot.messages.find((message) => message.role === "assistant" && message.turnId === turn.turnId)?.body;
    const task = turn.prompt || "Operator turn";
    return `
      <div class="subagent-card ${cssToken(turn.status)}">
        <div class="sa-header">
          <span class="sa-status-dot ${cssToken(turn.status)}"></span>
          <span class="sa-task">${esc(task.slice(0, 80))}${task.length > 80 ? "…" : ""}</span>
          <span class="sa-meta">${esc(routeLabel(route))}</span>
          <span class="sa-time">${timeFmt(Date.parse(turn.updatedAt || turn.startedAt || ""))}</span>
        </div>
        ${output ? `<div class="sa-output">${esc(output)}</div>` : ""}
      </div>
    `;
  }).join("");
}

function renderAgents(): string {
  const dispatchDisabled = operatorClient.state().status !== "ready";
  return `
    <div class="view agents-view">
      <div class="view-header">
        <h2>Operator turns</h2>
        <p class="muted">Durable tasks routed per call to the cheapest provider above the required quality floor.</p>
      </div>

      <div class="dispatch-quick">
        <textarea id="dispatch-input" rows="2" placeholder="Describe a task for Heiwa to route to the right local/provider-owned executor…"></textarea>
        <div class="dispatch-controls auto-route-controls">
          <div class="route-note">
            <strong>Auto-routed</strong>
            <span>Heiwa chooses the smallest sufficient route by task, privacy, quota, device state, and evidence quality.</span>
          </div>
          <button id="dispatch-btn" class="btn-primary" ${dispatchDisabled ? "disabled" : ""}>Dispatch</button>
        </div>
      </div>

      <div class="subagent-list">
        ${renderTurnRows()}
      </div>
    </div>
  `;
}

// ═══════════════════════════════════════════════════════════════════════════
// Render: browser view
// ═══════════════════════════════════════════════════════════════════════════

function renderBrowser(): string {
  return `
    <div class="view browser-view">
      <div class="browser-bar">
        <input id="browser-url" type="text" value="https://example.com" placeholder="Enter URL…" />
        <button id="browser-go" class="btn-primary">Go</button>
      </div>
      <div class="browser-frame">
        <iframe title="Heiwa browser" src="https://example.com" sandbox="allow-forms allow-same-origin allow-scripts allow-popups"></iframe>
      </div>
    </div>
  `;
}

// ═══════════════════════════════════════════════════════════════════════════
// Render: files view
// ═══════════════════════════════════════════════════════════════════════════

function renderFiles(): string {
  return `
    <div class="view files-view">
      <div class="view-header">
        <h2>Files</h2>
        <p class="muted">Browse the local filesystem.</p>
      </div>
      <div class="files-placeholder">
        <p class="muted">File browser loads from <code>/api/v1/files/tree</code>.</p>
      </div>
    </div>
  `;
}

function renderFeatureWindow(kind: "mail" | "finance" | "social"): string {
  const copy = subApps().find((app) => app.id === kind)!;
  const rows = [
    ["agent", copy.agent],
    ["server", copy.server],
    ["pane", copy.pinnedPane],
    ["skills", copy.skills.join(" · ")],
    ["tools", copy.tools.join(" · ")],
    ["personal", copy.personalization.join(" · ")],
  ];

  return `
    <div class="view feature-window">
      <section class="feature-panel">
        <header>
          <span>${copy.title}</span>
          <strong>${copy.state}</strong>
        </header>
        ${rows.map(([label, value]) => `
          <div class="feature-row">
            <span>${label}</span>
            <strong>${value}</strong>
          </div>
        `).join("")}
      </section>
    </div>
  `;
}

async function loadForView(nextView: View): Promise<void> {
  if (nextView === "home" || nextView === "windows") {
    await Promise.all([loadCalendar(), loadInbox(), loadHerd(), loadHerdCommands()]);
    if (nextView === "windows") await loadSelectedPaneText();
  }
  if (nextView === "calendar") await loadCalendar();
  if (nextView === "mail") await loadInbox();
}

function refreshOperatorUi(): void {
  const disabled = operatorClient.state().status !== "ready";
  const sendButton = document.querySelector<HTMLButtonElement>("#send-btn");
  if (sendButton) {
    sendButton.disabled = disabled;
    sendButton.textContent = disabled ? "…" : "↑";
  }
  const dispatchButton = document.querySelector<HTMLButtonElement>("#dispatch-btn");
  if (dispatchButton) dispatchButton.disabled = disabled;

  for (const viewId of ["chat", "agents"] as const) {
    const preview = dockItems().find((item) => item.id === viewId)?.preview();
    const target = document.querySelector<HTMLElement>(`.rail-btn[data-view="${viewId}"] .dock-preview`);
    if (preview && target) {
      target.innerHTML = `<strong>${esc(preview.title)}</strong>${preview.lines.map((line) => `<span>${esc(line)}</span>`).join("")}`;
    }
  }

  if (view === "chat") {
    const chat = document.querySelector<HTMLElement>("#chat-messages");
    if (chat) {
      chat.innerHTML = renderMessages();
      chat.scrollTop = chat.scrollHeight;
    }
  }
  if (view === "agents") {
    const list = document.querySelector<HTMLElement>(".subagent-list");
    if (list) list.innerHTML = renderTurnRows();
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// Main render
// ═══════════════════════════════════════════════════════════════════════════

function render(): void {
  const operatorStatus = operatorClient.state().status;
  const composerDisabled = operatorStatus !== "ready";
  const main =
    view === "home" ? renderHome() :
    view === "chat" ? renderChat() :
    view === "windows" ? renderWindows() :
    view === "calendar" ? renderCalendar() :
    view === "agents" ? renderAgents() :
    view === "mail" ? renderFeatureWindow("mail") :
    view === "finance" ? renderFeatureWindow("finance") :
    view === "social" ? renderFeatureWindow("social") :
    view === "browser" ? renderBrowser() :
    renderFiles();

  app.innerHTML = `
    <div class="app-shell">
      ${renderRail()}
      <main class="main-area">
        ${main}
        <div class="composer-area">
          <div class="composer-wrap">
            <textarea id="chat-input" rows="1" placeholder="Message Heiwa…"></textarea>
            <button id="send-btn" ${composerDisabled ? "disabled" : ""}>${composerDisabled ? "…" : "↑"}</button>
          </div>
          <div class="composer-hint">
            <span class="hint">${esc(viewCaption(view))}</span>
            <span class="hint">${esc(runtimeVersion(health))} · ${providersFromSnapshot(health).filter(p => p.status === "connected").length} providers</span>
          </div>
        </div>
      </main>
    </div>
  `;

  // Rail navigation
  document.querySelectorAll<HTMLButtonElement>(".rail-btn").forEach(btn => {
    btn.addEventListener("click", async () => {
      const nextView = btn.dataset.view as View;
      view = nextView;
      await loadForView(nextView);
      render();
    });
  });

  document.querySelectorAll<HTMLButtonElement>(".home-jump").forEach(btn => {
    btn.addEventListener("click", async () => {
      const nextView = btn.dataset.view as View;
      if (btn.dataset.pane && livePaneIds().has(btn.dataset.pane)) {
        selectedPane = btn.dataset.pane;
      }
      view = nextView;
      await loadForView(nextView);
      render();
    });
  });

  document.querySelectorAll<HTMLButtonElement>(".pane-row").forEach(btn => {
    btn.addEventListener("click", async () => {
      selectedPane = btn.dataset.pane ?? selectedPane;
      await loadSelectedPaneText();
      render();
    });
  });

  document.querySelector<HTMLButtonElement>("#refresh-herd")?.addEventListener("click", async () => {
    await loadHerd();
    await loadSelectedPaneText();
    render();
  });

  const paneModeSelect = document.querySelector<HTMLSelectElement>("#pane-mode");
  paneModeSelect?.addEventListener("change", () => {
    paneMode = paneModeSelect.value === "run" ? "run" : "send";
    render();
  });

  const submitPaneCommand = async () => {
    const pane = selectedPaneRow();
    const input = document.querySelector<HTMLInputElement | HTMLSelectElement>("#pane-command");
    const text = input?.value.trim() ?? "";
    if (!pane || !livePaneIds().has(pane.pane) || !text) return;
    if (input instanceof HTMLInputElement) input.value = "";
    await runPaneAction(() => paneMode === "run"
      ? runHerdPane(pane.pane, text)
      : sendHerdPane(pane.pane, text)
    );
  };

  document.querySelector<HTMLButtonElement>("#pane-submit")?.addEventListener("click", submitPaneCommand);
  document.querySelector<HTMLInputElement>("#pane-command")?.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      submitPaneCommand();
    }
  });

  document.querySelector<HTMLButtonElement>("#pane-read")?.addEventListener("click", async () => {
    await loadSelectedPaneText();
    render();
  });
  document.querySelector<HTMLButtonElement>("#pane-focus")?.addEventListener("click", async () => {
    const pane = selectedPaneRow();
    if (pane) await runPaneAction(() => focusHerdPane(pane.pane));
  });
  document.querySelector<HTMLButtonElement>("#pane-split-right")?.addEventListener("click", async () => {
    const pane = selectedPaneRow();
    if (pane) await runPaneAction(() => splitHerdPane(pane.pane, "right", pane.cwd));
  });
  document.querySelector<HTMLButtonElement>("#pane-split-down")?.addEventListener("click", async () => {
    const pane = selectedPaneRow();
    if (pane) await runPaneAction(() => splitHerdPane(pane.pane, "down", pane.cwd));
  });

  document.querySelectorAll<HTMLButtonElement>("[data-cal-nav]").forEach(btn => {
    btn.addEventListener("click", () => {
      const year = calendarDate.getFullYear();
      const month = calendarDate.getMonth();
      const action = btn.dataset.calNav;
      if (action === "prev") calendarDate = new Date(year, month - 1, 1);
      if (action === "next") calendarDate = new Date(year, month + 1, 1);
      if (action === "today") calendarDate = new Date();
      render();
    });
  });

  // Chat input
  const input = document.querySelector<HTMLTextAreaElement>("#chat-input");
  const sendBtn = document.querySelector<HTMLButtonElement>("#send-btn");

  const doSend = async () => {
    const text = input?.value.trim() ?? "";
    if (!text || operatorClient.state().status !== "ready") return;
    if (input) input.value = "";
    view = "chat";
    render();
    await sendChat(text).catch(() => undefined);
    render();
  };

  input?.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); doSend(); }
  });
  sendBtn?.addEventListener("click", doSend);

  // Auto-resize textarea
  input?.addEventListener("input", () => {
    if (input) {
      input.style.height = "auto";
      input.style.height = Math.min(input.scrollHeight, 120) + "px";
    }
  });

  // Dispatch form (agents view)
  const dispatchBtn = document.querySelector<HTMLButtonElement>("#dispatch-btn");
  dispatchBtn?.addEventListener("click", async () => {
    const task = document.querySelector<HTMLTextAreaElement>("#dispatch-input")?.value.trim() ?? "";
    if (!task || operatorClient.state().status !== "ready") return;
    await sendChat(task).catch(() => undefined);
    render();
  });

  // Browser bar
  const browserGo = document.querySelector<HTMLButtonElement>("#browser-go");
  const browserUrl = document.querySelector<HTMLInputElement>("#browser-url");
  browserGo?.addEventListener("click", () => {
    const url = browserUrl?.value.trim() || "https://example.com";
    const iframe = document.querySelector<HTMLIFrameElement>(".browser-frame iframe");
    if (iframe) iframe.src = url;
  });
  browserUrl?.addEventListener("keydown", (e) => {
    if (e.key === "Enter") browserGo?.click();
  });

  // Scroll chat to bottom
  const chatMsgs = document.querySelector("#chat-messages");
  if (chatMsgs) chatMsgs.scrollTop = chatMsgs.scrollHeight;
}

// ═══════════════════════════════════════════════════════════════════════════
// Boot
// ═══════════════════════════════════════════════════════════════════════════

async function boot(): Promise<void> {
  render();
  await loadHealth();
  await Promise.all([loadCalendar(), loadInbox(), loadHerd(), loadHerdCommands()]);
  await operatorClient.start(OPERATOR_THREAD_ID);
  connectWS();
  render();
}

void boot();
