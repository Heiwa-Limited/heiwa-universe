import { A, useLocation } from "@solidjs/router";
import { For, Show, createMemo, createSignal } from "solid-js";
import type { JSX, ParentProps } from "solid-js";

type NavItem = {
  href: string;
  label: string;
  mark: string;
};

type InspectorTab = "telemetry" | "safety" | "checklist" | "trace";

type SurfaceSignal = {
  name: string;
  status: string;
  detail: string;
  tone: "online" | "active" | "secure";
};

const primaryNav: NavItem[] = [
  { href: "/", label: "Dashboard", mark: "D" },
  { href: "/inbox", label: "Inbox", mark: "I" },
  { href: "/routes", label: "Routes", mark: "R" },
  { href: "/live", label: "Live", mark: "L" },
  { href: "/repl", label: "REPL", mark: ">" },
  { href: "/providers", label: "Providers", mark: "P" },
  { href: "/connections", label: "Connections", mark: "C" },
  { href: "/status", label: "Status", mark: "S" },
];

const evidenceNav: NavItem[] = [
  { href: "/missions", label: "Missions", mark: "M" },
  { href: "/approvals", label: "Approvals", mark: "A" },
  { href: "/history", label: "History", mark: "H" },
  { href: "/traces", label: "Traces", mark: "T" },
  { href: "/memory", label: "Memory", mark: "K" },
];

const systemNav: NavItem[] = [
  { href: "/agents", label: "Agents", mark: "G" },
  { href: "/hooks", label: "Hooks", mark: "U" },
  { href: "/crons", label: "Crons", mark: "Q" },
  { href: "/rate-groups", label: "Rate groups", mark: "$" },
  { href: "/cells", label: "Cells", mark: "#" },
  { href: "/domains", label: "Domains", mark: "N" },
  { href: "/governance", label: "Governance", mark: "V" },
];

const threadList = [
  { title: "Release sandbox gate", time: "now", active: true },
  { title: "Inbox read model spine", time: "12m", active: false },
  { title: "Provider auth parity", time: "41m", active: false },
  { title: "Cloudflare install source", time: "1h", active: false },
];

const checklist = [
  { text: "Local runtime source labeled", done: true },
  { text: "GitHub release path dry-run", done: true },
  { text: "Cockpit shell active", done: true },
  { text: "Visual build verified", done: false },
  { text: "Browser proof captured", done: false },
];

const traceLines = [
  { kind: "system", text: "runtime: ~/.heiwa state detected" },
  { kind: "drex", text: "plane: intake -> execution -> evidence" },
  { kind: "api", text: "source: GitHub Releases preferred" },
  { kind: "system", text: "mode: checkout-dev visual verification" },
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
    detail: "GitHub, Cloudflare, STDB",
    tone: "secure",
  },
];

function routeTitle(pathname: string): string {
  if (pathname === "/") return "Dashboard";
  const match = [...primaryNav, ...evidenceNav, ...systemNav].find(
    (item) => item.href === pathname,
  );
  return match?.label ?? "Workspace";
}

function SidebarSection(props: { title: string; items: NavItem[] }): JSX.Element {
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
  const [inspectorTab, setInspectorTab] = createSignal<InspectorTab>("telemetry");
  const currentTitle = createMemo(() => routeTitle(location.pathname));

  return (
    <div class="heiwa-app-layout">
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
            <button class="btn-new-thread" type="button" aria-label="New thread">
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
          <div class="header-breadcrumb" aria-label="Current workspace">
            <span class="bc-root">Heiwa</span>
            <span class="bc-sep">/</span>
            <span class="bc-active">{currentTitle()}</span>
          </div>
          <div class="header-actions">
            <button class="btn-workspace-action" type="button">
              Local runtime
            </button>
            <button class="btn-workspace-action glow" type="button">
              Evidence view
            </button>
          </div>
        </header>
        <div class="workspace-viewport">{props.children}</div>
      </main>

      <aside class="heiwa-sidecar" aria-label="Runtime inspector">
        <div class="sidecar-tabs">
          <For
            each={[
              ["telemetry", "Telemetry"],
              ["safety", "Safety"],
              ["checklist", "Tasks"],
              ["trace", "Trace"],
            ] as const}
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
                        <span class={`status-pill ${surface.tone}`}>{surface.status}</span>
                      </div>
                    )}
                  </For>
                </div>
              </div>

              <div class="telemetry-widget">
                <span class="widget-label">CPU</span>
                <div class="widget-value-row">
                  <strong>14%</strong>
                  <span class="status-pill online">steady</span>
                </div>
                <div class="telemetry-bar">
                  <div class="telemetry-bar-fill cyan" style={{ width: "14%" }} />
                </div>
                <span class="widget-desc">Background loop under budget</span>
              </div>

              <div class="telemetry-widget">
                <span class="widget-label">Memory</span>
                <div class="widget-value-row">
                  <strong>44%</strong>
                  <span class="status-pill active">local</span>
                </div>
                <div class="telemetry-bar">
                  <div class="telemetry-bar-fill gold" style={{ width: "44%" }} />
                </div>
                <span class="widget-desc">State root: ~/.heiwa/state</span>
              </div>

              <div class="telemetry-widget">
                <span class="widget-label">VRAM</span>
                <div class="widget-value-row">
                  <strong>5.8 GB</strong>
                  <span class="status-pill online">Ollama</span>
                </div>
                <div class="telemetry-bar">
                  <div class="telemetry-bar-fill magenta" style={{ width: "36%" }} />
                </div>
                <span class="widget-desc">Local model lane available</span>
              </div>
            </section>
          </Show>

          <Show when={inspectorTab() === "safety"}>
            <section class="safety-panel">
              <div class="policy-card">
                <span class="policy-header">Autonomy boundary</span>
                <p class="policy-text">
                  Local analysis and drafts can run; external side effects stay approval-gated.
                </p>
                <span class="policy-badge enforced">enforced</span>
              </div>
              <div class="policy-card">
                <span class="policy-header">Install authority</span>
                <p class="policy-text">
                  GitHub Releases are the user update source. Checkout reinstall is developer-only.
                </p>
                <span class="policy-badge active">active</span>
              </div>
              <div class="policy-card">
                <span class="policy-header">Evidence sync</span>
                <p class="policy-text">
                  Local state records current truth; STDB sync is a backend path when enabled.
                </p>
                <span class="policy-badge enforced">local-first</span>
              </div>
            </section>
          </Show>

          <Show when={inspectorTab() === "checklist"}>
            <section class="progress-panel">
              <div class="progress-header-row">
                <span class="progress-title">Execution checklist</span>
                <span class="progress-pct">60%</span>
              </div>
              <div class="progress-list">
                <For each={checklist}>
                  {(item) => (
                    <div class="checklist-item" classList={{ done: item.done }}>
                      <span class="checkbox-box" aria-hidden="true">
                        {item.done ? "✓" : ""}
                      </span>
                      <span class="checklist-text">{item.text}</span>
                    </div>
                  )}
                </For>
              </div>
            </section>
          </Show>

          <Show when={inspectorTab() === "trace"}>
            <section class="trace-panel">
              <span class="trace-header">Runtime trace</span>
              <div class="trace-log-viewport">
                <For each={traceLines}>
                  {(line) => (
                    <div class={`trace-log-line ${line.kind}`}>{line.text}</div>
                  )}
                </For>
              </div>
            </section>
          </Show>
        </div>
      </aside>
    </div>
  );
}
