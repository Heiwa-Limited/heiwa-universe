import { A, useLocation } from "@solidjs/router";
import type { JSX, ParentProps } from "solid-js";
import {
  createMemo,
  createResource,
  createSignal,
  For,
  onCleanup,
  onMount,
  Show,
} from "solid-js";
import { v1 } from "./lib/endpoints";

type NavItem = {
  href: string;
  label: string;
  mark: string;
};

type InspectorTab =
  | "telemetry"
  | "personalize"
  | "projects"
  | "advanced"
  | "settings";

type SurfaceSignal = {
  name: string;
  status: string;
  detail: string;
  tone: "online" | "active" | "secure";
};

type ConsoleItem = {
  name: string;
  detail: string;
  status: string;
};

const primaryNav: NavItem[] = [
  { href: "/", label: "Run", mark: "▶" },
  { href: "/today", label: "Today", mark: "☀" },
  { href: "/inbox", label: "Inbox", mark: "↯" },
  { href: "/calendar", label: "Calendar", mark: "◷" },
  { href: "/mail", label: "Mail", mark: "@" },
];

const evidenceNav: NavItem[] = [
  { href: "/approvals", label: "Approvals", mark: "✓" },
  { href: "/receipts", label: "Receipts", mark: "≡" },
  { href: "/history", label: "History", mark: "H" },
  { href: "/traces", label: "Traces", mark: "T" },
  { href: "/memory", label: "Memory", mark: "M" },
];

const systemNav: NavItem[] = [
  { href: "/automations", label: "Automations", mark: "⟳" },
  { href: "/advanced", label: "Dashboard", mark: "⌘" },
  { href: "/connections", label: "Connections", mark: "K" },
  { href: "/providers", label: "Providers", mark: "P" },
  { href: "/routes", label: "Routes", mark: "R" },
  { href: "/model-matrix", label: "Model Matrix", mark: "X" },
  { href: "/repl", label: "REPL", mark: ">" },
  { href: "/status", label: "Status", mark: "S" },
];

const threadList = [
  { title: "Release sandbox gate", time: "now", active: true },
  { title: "Inbox read model spine", time: "12m", active: false },
  { title: "Provider auth parity", time: "41m", active: false },
  { title: "Cloudflare install source", time: "1h", active: false },
];

const traceLines = [
  { kind: "system", text: "runtime: ~/.heiwa state detected" },
  { kind: "drex", text: "plane: intake/execution/evidence" },
  { kind: "api", text: "source: GitHub Releases preferred" },
  { kind: "system", text: "mode: checkout-dev visual QA" },
];

const surfaceSignals: SurfaceSignal[] = [
  {
    name: "Browser",
    status: "attached",
    detail: "Dev browser evidence stream",
    tone: "online",
  },
  {
    name: "Mail + calendar",
    status: "metadata",
    detail: "Priority summaries only",
    tone: "secure",
  },
  {
    name: "Machine",
    status: "live",
    detail: "CPU, memory, local runtime",
    tone: "online",
  },
  {
    name: "Computer use",
    status: "approval",
    detail: "Staged before side effects",
    tone: "active",
  },
  {
    name: "Integrations",
    status: "staged",
    detail: "GitHub, Cloudflare, Lance",
    tone: "secure",
  },
];

const personalizationItems: ConsoleItem[] = [
  {
    name: "Skills",
    detail: "User-approved reusable behaviors and task modes",
    status: "curated",
  },
  {
    name: "Rules",
    detail: "Autonomy boundaries, safety gates, and style defaults",
    status: "enforced",
  },
  {
    name: "Preferences",
    detail: "Communication, routing, notification, and UI defaults",
    status: "local",
  },
  {
    name: "Connectors",
    detail: "OAuth, CLI, local, and API surfaces with revocation state",
    status: "gated",
  },
];

const projectItems: ConsoleItem[] = [
  {
    name: "Heiwa universe",
    detail: "Source, releases, docs, app client, and local runtime work",
    status: "active",
  },
  {
    name: "Runtime install",
    detail: "GitHub release source, local sandbox, update receipts",
    status: "managed",
  },
  {
    name: "Read model spine",
    detail: "Inbox, history, traces, memory, approvals, telemetry",
    status: "queued",
  },
];

const advancedItems: ConsoleItem[] = [
  {
    name: "Routing policy",
    detail: "Local-first provider selection, quotas, and fallbacks",
    status: "inspect",
  },
  {
    name: "Evidence sync",
    detail: "Local JSONL evidence is the authority; GitHub sync is planned",
    status: "secure",
  },
  {
    name: "Runtime bridge",
    detail: "Codex, Claude, Gemini, Antigravity, Ollama, MCP tools",
    status: "bridge",
  },
];

function routeTitle(pathname: string): string {
  if (pathname === "/") return "Run";
  if (pathname === "/advanced" || pathname === "/dashboard")
    return "Dashboard · Advanced settings";
  const match = [...primaryNav, ...evidenceNav, ...systemNav].find(
    (item) => item.href === pathname,
  );
  return match?.label ?? "Workspace";
}

const TELEMETRY_POLL_MS = 5000;

/** Live runtime telemetry from /api/v1/resource, polled while visible. */
function LiveTelemetryPanel(): JSX.Element {
  const [resource, { refetch }] = createResource(() => v1.resource());

  onMount(() => {
    const timer = setInterval(() => void refetch(), TELEMETRY_POLL_MS);
    onCleanup(() => clearInterval(timer));
  });

  const loadPercent = createMemo(() => {
    const snapshot = resource()?.snapshot;
    if (!snapshot || snapshot.cpu_count === 0) return null;
    return Math.min(
      100,
      Math.round((snapshot.load_1m / snapshot.cpu_count) * 100),
    );
  });

  const freeGb = createMemo(() => {
    const snapshot = resource()?.snapshot;
    if (!snapshot) return null;
    return snapshot.free_memory_bytes / 1024 ** 3;
  });

  const localLane = createMemo(() => {
    const admissions = resource()?.admissions;
    if (!admissions) return null;
    if (admissions.local_model_large) return "large + small";
    if (admissions.local_model_small) return "small only";
    return "blocked";
  });

  return (
    <section class="telemetry-panel">
      <div class="surface-watch-panel">
        <div class="surface-watch-header">
          <span class="widget-label">Connected surfaces</span>
          <span class="status-pill online">watching</span>
        </div>
        <div class="surface-signal-list">
          <For each={surfaceSignals}>
            {(surface) => (
              <div class="surface-signal-row">
                <div>
                  <span class="surface-name">{surface.name}</span>
                  <span class="surface-detail">{surface.detail}</span>
                </div>
                <span class={`status-pill ${surface.tone}`}>
                  {surface.status}
                </span>
              </div>
            )}
          </For>
        </div>
      </div>

      <Show
        when={resource()}
        fallback={<span class="widget-desc">Probing local runtime…</span>}
      >
        <div class="telemetry-widget">
          <span class="widget-label">CPU load (1m)</span>
          <div class="widget-value-row">
            <strong>
              {resource()?.snapshot.load_1m.toFixed(2)} /{" "}
              {resource()?.snapshot.cpu_count} cores
            </strong>
            <span
              class={`status-pill ${(loadPercent() ?? 0) > 80 ? "active" : "online"}`}
            >
              {(loadPercent() ?? 0) > 80 ? "hot" : "steady"}
            </span>
          </div>
          <div class="telemetry-bar">
            <div
              class="telemetry-bar-fill cyan"
              style={{ width: `${loadPercent() ?? 0}%` }}
            />
          </div>
          <span class="widget-desc">source: {resource()?.sources.load_1m}</span>
        </div>

        <div class="telemetry-widget">
          <span class="widget-label">Free memory</span>
          <div class="widget-value-row">
            <strong>{freeGb()?.toFixed(1)} GB</strong>
            <span class="status-pill active">local</span>
          </div>
          <div class="telemetry-bar">
            <div
              class="telemetry-bar-fill gold"
              style={{
                width: `${Math.min(100, Math.round(((freeGb() ?? 0) / 24) * 100))}%`,
              }}
            />
          </div>
          <span class="widget-desc">
            source: {resource()?.sources.free_memory_bytes}
          </span>
        </div>

        <div class="telemetry-widget">
          <span class="widget-label">Local model lane</span>
          <div class="widget-value-row">
            <strong>{localLane()}</strong>
            <span
              class={`status-pill ${localLane() === "blocked" ? "active" : "online"}`}
            >
              {localLane() === "blocked" ? "gated" : "admitted"}
            </span>
          </div>
          <span class="widget-desc">
            resource policy gates always-on local work
          </span>
        </div>
      </Show>
    </section>
  );
}

function SidebarSection(props: {
  title: string;
  items: NavItem[];
}): JSX.Element {
  return (
    <section class="sidebar-section">
      <div class="section-header-row">
        <span class="section-title">{props.title}</span>
      </div>
      <nav class="sidebar-nav" aria-label={props.title}>
        <For each={props.items}>
          {(item) => (
            <A
              class="nav-item"
              href={item.href}
              end={item.href === "/"}
              activeClass="active"
            >
              <span class="nav-mark" aria-hidden="true">
                {item.mark}
              </span>
              <span>{item.label}</span>
            </A>
          )}
        </For>
      </nav>
    </section>
  );
}

export default function App(props: ParentProps): JSX.Element {
  const location = useLocation();
  const [inspectorTab, setInspectorTab] =
    createSignal<InspectorTab>("telemetry");
  const currentTitle = createMemo(() => routeTitle(location.pathname));

  return (
    <div
      class="heiwa-app-layout"
      classList={{ "main-app-view": location.pathname === "/" }}
    >
      <aside class="heiwa-sidebar" aria-label="Heiwa workspace navigation">
        <div class="sidebar-header">
          <A class="brand-logo" href="/">
            <span class="logo-text">Heiwa</span>
            <span class="logo-sub">.app</span>
          </A>
          <div class="active-project">
            <span class="project-indicator" aria-hidden="true" />
            <span class="project-name">~/heiwa-universe</span>
          </div>
        </div>

        <SidebarSection title="Operate" items={primaryNav} />
        <SidebarSection title="Evidence" items={evidenceNav} />
        <SidebarSection title="System" items={systemNav} />

        <section class="sidebar-section session-section">
          <div class="section-header-row">
            <span class="section-title">Threads</span>
            <button
              class="btn-new-thread"
              type="button"
              aria-label="New thread"
            >
              +
            </button>
          </div>
          <div class="session-thread-list">
            <For each={threadList}>
              {(thread) => (
                <button
                  type="button"
                  class="thread-item"
                  classList={{ active: thread.active }}
                >
                  <span class="thread-title">{thread.title}</span>
                  <span class="thread-time">{thread.time}</span>
                </button>
              )}
            </For>
          </div>
        </section>

        <div class="sidebar-footer">
          <div class="operator-profile">
            <div class="avatar" aria-hidden="true">
              D
            </div>
            <div class="operator-info">
              <span class="operator-name">Devon</span>
              <span class="operator-role">Owner runtime</span>
            </div>
          </div>
        </div>
      </aside>

      <main class="heiwa-main-content">
        <header class="workspace-header">
          <nav class="header-breadcrumb" aria-label="Current workspace">
            <span class="bc-root">Heiwa</span>
            <span class="bc-sep">/</span>
            <span class="bc-active">{currentTitle()}</span>
          </nav>
          <div class="header-actions">
            <A class="btn-workspace-action" href="/">
              Run workspace
            </A>
            <A class="btn-workspace-action glow" href="/advanced">
              Advanced dashboard
            </A>
          </div>
        </header>
        <div class="workspace-viewport">{props.children}</div>
      </main>

      <aside
        class="heiwa-sidecar"
        aria-label="Browser console and runtime inspector"
      >
        <div class="sidecar-console-header">
          <div>
            <span class="console-eyebrow">Browser console</span>
            <strong>Configuration and ops</strong>
          </div>
          <span class="status-pill secure">secondary</span>
        </div>

        <div class="sidecar-tabs">
          <For
            each={
              [
                ["telemetry", "Telemetry"],
                ["personalize", "Personalize"],
                ["projects", "Projects"],
                ["advanced", "Advanced"],
                ["settings", "Settings"],
              ] as const
            }
          >
            {([id, label]) => (
              <button
                type="button"
                class="sidecar-tab-btn"
                classList={{ active: inspectorTab() === id }}
                aria-pressed={inspectorTab() === id}
                onClick={() => setInspectorTab(id)}
              >
                {label}
              </button>
            )}
          </For>
        </div>

        <div class="sidecar-body">
          <Show when={inspectorTab() === "telemetry"}>
            <LiveTelemetryPanel />
          </Show>

          <Show when={inspectorTab() === "personalize"}>
            <section class="console-panel">
              <div class="console-panel-copy">
                <span class="widget-label">Personalization</span>
                <p>
                  Customize how Heiwa understands, routes, and presents work.
                </p>
              </div>
              <For each={personalizationItems}>
                {(item) => (
                  <div class="policy-card console-card">
                    <span class="policy-header">{item.name}</span>
                    <p class="policy-text">{item.detail}</p>
                    <span class="policy-badge active">{item.status}</span>
                  </div>
                )}
              </For>
            </section>
          </Show>

          <Show when={inspectorTab() === "projects"}>
            <section class="console-panel">
              <div class="console-panel-copy">
                <span class="widget-label">Auto-managed projects</span>
                <p>
                  Heiwa should group work by durable projects without making the
                  user file tickets.
                </p>
              </div>
              <For each={projectItems}>
                {(item) => (
                  <div class="policy-card console-card">
                    <span class="policy-header">{item.name}</span>
                    <p class="policy-text">{item.detail}</p>
                    <span class="policy-badge enforced">{item.status}</span>
                  </div>
                )}
              </For>
            </section>
          </Show>

          <Show when={inspectorTab() === "advanced"}>
            <section class="console-panel">
              <div class="console-panel-copy">
                <span class="widget-label">Advanced settings</span>
                <p>
                  Low-level controls stay here so the main app remains one clean
                  conversation.
                </p>
              </div>
              <For each={advancedItems}>
                {(item) => (
                  <div class="policy-card console-card">
                    <span class="policy-header">{item.name}</span>
                    <p class="policy-text">{item.detail}</p>
                    <span class="policy-badge enforced">{item.status}</span>
                  </div>
                )}
              </For>
            </section>
          </Show>

          <Show when={inspectorTab() === "settings"}>
            <section class="console-panel">
              <div class="console-panel-copy">
                <span class="widget-label">Open surfaces</span>
                <p>
                  Jump between the app dashboard, browser view, and runtime
                  settings.
                </p>
              </div>
              <div class="console-link-grid">
                <A class="console-link-card" href="/">
                  <span>Browser console</span>
                  <small>Per-user support dashboard</small>
                </A>
                <A class="console-link-card" href="/status">
                  <span>App settings</span>
                  <small>Runtime health and install state</small>
                </A>
                <A class="console-link-card" href="/connections">
                  <span>Connectors</span>
                  <small>Provider and account surfaces</small>
                </A>
                <A class="console-link-card" href="/governance">
                  <span>Governance</span>
                  <small>Policy and public truth</small>
                </A>
              </div>
              <div class="trace-panel compact">
                <span class="trace-header">Runtime trace</span>
                <div class="trace-log-viewport">
                  <For each={traceLines}>
                    {(line) => (
                      <div class={`trace-log-line ${line.kind}`}>
                        {line.text}
                      </div>
                    )}
                  </For>
                </div>
              </div>
            </section>
          </Show>
        </div>
      </aside>
    </div>
  );
}
