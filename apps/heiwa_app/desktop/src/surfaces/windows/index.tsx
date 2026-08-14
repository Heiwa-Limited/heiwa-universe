import { For, Show } from "solid-js";
import { cssToken, shortenPath } from "../../lib/format";
import { useApp } from "../../state/app";
import type { SurfaceModule } from "../types";
import "./windows.css";

function WindowsSurface() {
  const app = useApp();
  const herd = app.herd;

  const activePane = () => {
    const panes = herd.panes();
    return panes.find((pane) => pane.pane === herd.selectedPane()) ?? panes[0] ?? null;
  };
  const canControl = () => {
    const pane = activePane();
    return Boolean(pane && herd.livePaneIds().has(pane.pane));
  };

  let commandInput: HTMLInputElement | undefined;
  let commandSelect: HTMLSelectElement | undefined;

  const submitCommand = async () => {
    const pane = activePane();
    const text = (herd.mode() === "run" ? commandSelect?.value : commandInput?.value)?.trim() ?? "";
    if (!pane || !herd.livePaneIds().has(pane.pane) || !text) return;
    if (commandInput) commandInput.value = "";
    await herd.act(() =>
      herd.mode() === "run" ? herd.run(pane.pane, text) : herd.send(pane.pane, text),
    );
  };

  return (
    <div class="view windows-view">
      <div class="view-header">
        <h2>Windows</h2>
        <p class="muted">
          Pinned terminal panes, ops servers, and app agents over one runtime state.
        </p>
      </div>

      <div class="terminal-cockpit">
        <section class="panel pane-list-panel">
          <header>
            <span>Terminal panes</span>
            <strong>{herd.panes().length}</strong>
          </header>
          <div class="pane-list">
            <Show
              when={herd.panes().length}
              fallback={<div class="empty-state">No live panes reported by herdr.</div>}
            >
              <For each={herd.panes()}>
                {(pane) => (
                  <button
                    class="pane-row"
                    classList={{ selected: activePane()?.pane === pane.pane }}
                    onClick={async () => {
                      herd.select(pane.pane);
                      await herd.loadPaneText();
                    }}
                  >
                    <span class="pane-row-main">
                      <strong>{pane.agent === "-" ? pane.workspace : pane.agent}</strong>
                      <span class="quiet">
                        {pane.pane} · {shortenPath(pane.cwd)}
                      </span>
                    </span>
                    <span class={`state-chip ${cssToken(pane.state)}`}>{pane.state}</span>
                  </button>
                )}
              </For>
            </Show>
          </div>
          <button
            class="small-action"
            onClick={async () => {
              await herd.load();
              await herd.loadPaneText();
            }}
          >
            Refresh
          </button>
        </section>

        <section class="panel terminal-panel">
          <header>
            <span>{activePane()?.pane ?? "No pane"}</span>
            <strong>{herd.paneStatus() || (canControl() ? "ready" : "no live pane")}</strong>
          </header>
          <pre class="terminal-output">
            {herd.paneText() ||
              (canControl()
                ? "Select refresh to read pane output."
                : "No live pane output. Planned panes are placeholders until herdr reports real panes.")}
          </pre>

          <div class="pane-controls">
            <select
              disabled={!canControl()}
              value={herd.mode()}
              onChange={(event) => herd.setMode(event.currentTarget.value === "run" ? "run" : "send")}
            >
              <option value="send">send</option>
              <option value="run">run catalog</option>
            </select>

            <Show
              when={herd.mode() === "run" && herd.commands().length > 0}
              fallback={
                <input
                  ref={commandInput}
                  type="text"
                  disabled={!canControl()}
                  placeholder={herd.mode() === "run" ? "command id: git.status" : "prompt text…"}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") {
                      event.preventDefault();
                      void submitCommand();
                    }
                  }}
                />
              }
            >
              <select ref={commandSelect} disabled={!canControl()}>
                <For each={herd.commands()}>
                  {(command) => (
                    <option value={command.id}>
                      {command.label} · {command.id}
                    </option>
                  )}
                </For>
              </select>
            </Show>

            <button
              class="small-action primary-mini"
              disabled={!canControl()}
              onClick={() => void submitCommand()}
            >
              {herd.mode() === "run" ? "Run" : "Send"}
            </button>
          </div>

          <div class="pane-actions">
            <button
              class="small-action"
              disabled={!canControl()}
              onClick={() => void herd.loadPaneText()}
            >
              Read
            </button>
            <button
              class="small-action"
              disabled={!canControl()}
              onClick={() => {
                const pane = activePane();
                if (pane) void herd.act(() => herd.focus(pane.pane));
              }}
            >
              Focus
            </button>
            <button
              class="small-action"
              disabled={!canControl()}
              onClick={() => {
                const pane = activePane();
                if (pane) void herd.act(() => herd.split(pane.pane, "right", pane.cwd));
              }}
            >
              Split →
            </button>
            <button
              class="small-action"
              disabled={!canControl()}
              onClick={() => {
                const pane = activePane();
                if (pane) void herd.act(() => herd.split(pane.pane, "down", pane.cwd));
              }}
            >
              Split ↓
            </button>
          </div>

          <p class="pane-source quiet">
            source {herd.paneSource() || herd.source()} · herd {herd.status()} via {herd.source()}
          </p>
        </section>
      </div>

      <div class="multiplexer-grid">
        <section class="panel">
          <header>
            <span>Deno / herdr</span>
            <strong>{herd.status()}</strong>
          </header>
          <div class="mux-body">
            <div class="mux-line">
              <span>source</span>
              <strong>{herd.source()}</strong>
            </div>
            <div class="mux-line">
              <span>cli</span>
              <strong>herdr read/send/split/focus; run is catalog-only unless opted in</strong>
            </div>
            <div class="mux-line">
              <span>mode</span>
              <strong>native Tauri commands; terminal daemon remains authority target</strong>
            </div>
          </div>
        </section>

        <section class="panel">
          <header>
            <span>Sub-app agents</span>
            <strong>{app.subApps().length}</strong>
          </header>
          <div class="mux-body">
            <For each={app.subApps().slice(0, 5)}>
              {(sub) => (
                <div class="mux-line">
                  <span>{sub.title}</span>
                  <strong>
                    {sub.agent} · {sub.skills.join(" · ")}
                  </strong>
                </div>
              )}
            </For>
          </div>
        </section>

        <section class="panel">
          <header>
            <span>Ops personalization</span>
            <strong>
              {app.runtime.health()?.snapshot?.data?.approvals?.pending ??
                app.runtime.inbox().length}
            </strong>
          </header>
          <div class="mux-body">
            <For each={app.subApps().slice(0, 5)}>
              {(sub) => (
                <div class="mux-line">
                  <span>{sub.title}</span>
                  <strong>{sub.personalization.join(" · ")}</strong>
                </div>
              )}
            </For>
          </div>
        </section>
      </div>
    </div>
  );
}

export const windowsSurface: SurfaceModule = {
  id: "windows",
  label: "Windows",
  glyph: "▥",
  caption: "windows",
  Component: WindowsSurface,
  preview: (app) => ({
    title: "Multiplexer",
    lines: [
      `${app.herd.panes().length} live herdr panes`,
      "pinned ops windows",
      "terminal + sub-app servers",
    ],
  }),
  refresh: async (app) => {
    await Promise.all([
      app.runtime.loadCalendar(),
      app.runtime.loadInbox(),
      app.herd.load(),
      app.herd.loadCommands(),
    ]);
    await app.herd.loadPaneText();
  },
};
