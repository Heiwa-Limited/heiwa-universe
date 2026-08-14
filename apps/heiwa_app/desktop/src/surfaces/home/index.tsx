import { For } from "solid-js";
import { cssToken, shortenPath } from "../../lib/format";
import { useApp } from "../../state/app";
import type { HerdPane } from "../../runtime";
import type { SurfaceId } from "../ids";
import type { SurfaceModule } from "../types";
import "./home.css";

/**
 * Which surface a placeholder tile stands for.
 *
 * Matches on the workspace/pane/agent/cwd text because a planned pane names
 * its surface there ("calendar:today", "mail:triage") — there is no live
 * pane to key off yet.
 */
function surfaceForPane(pane: HerdPane): SurfaceId {
  const haystack =
    `${pane.workspace} ${pane.pane} ${pane.agent} ${pane.cwd}`.toLowerCase();
  if (haystack.includes("calendar")) return "calendar";
  if (haystack.includes("mail")) return "mail";
  if (haystack.includes("finance")) return "finance";
  if (haystack.includes("social")) return "social";
  if (haystack.includes("file")) return "files";
  if (haystack.includes("browser")) return "browser";
  if (haystack.includes("agent") || haystack.includes("worker")) return "workers";
  return "windows";
}

/** Placeholder pane rows shown before herdr reports live panes. */
function fallbackPanes(app: ReturnType<typeof useApp>): HerdPane[] {
  return [
    {
      workspace: "heiwa",
      pane: "herdr",
      agent: "multiplexer",
      state: app.herd.status(),
      cwd: "Heiwa.app native herd command",
      message: "live herd feed not connected",
    },
    ...app.subApps().slice(0, 5).map((sub) => ({
      workspace: sub.title.toLowerCase(),
      pane: sub.pinnedPane,
      agent: sub.agent,
      state: "planned",
      cwd: sub.server,
      message: sub.skills.join(" · "),
    })),
  ];
}

function HomeSurface() {
  const app = useApp();
  const visiblePanes = () => (app.herd.panes().length ? app.herd.panes() : fallbackPanes(app));
  const pinned = () => visiblePanes().slice(0, 8);

  return (
    <div class="view home-view">
      <section class="home-command">
        <div>
          <h1>Heiwa Ops</h1>
          <p class="muted">
            {pinned().length} pinned panes · herd {app.herd.status()} via {app.herd.source()} ·{" "}
            {app.subApps().length} sub-app agents
          </p>
        </div>
        <button class="btn-primary" onClick={() => app.navigate("windows")}>
          Open Windows
        </button>
      </section>

      <section class="pinned-pane-board" aria-label="Pinned terminal and ops panes">
        <For each={pinned()}>
          {(pane, index) => (
            <button
              class="pinned-pane"
              classList={{ "primary-pane": index() === 0 }}
              onClick={() => {
                // A live pane opens in Windows with that pane selected; a
                // placeholder tile opens the surface it stands for. Sending
                // every tile to Windows would dead-end the five sub-app
                // tiles that are all a fresh install shows.
                if (app.herd.livePaneIds().has(pane.pane)) {
                  app.herd.select(pane.pane);
                  app.navigate("windows");
                  void app.herd.loadPaneText();
                } else {
                  app.navigate(surfaceForPane(pane));
                }
              }}
            >
              <span class="pane-topline">
                <span>{pane.workspace}</span>
                <span class={`state-chip ${cssToken(pane.state)}`}>{pane.state}</span>
              </span>
              <strong>{pane.agent === "-" ? pane.pane : pane.agent}</strong>
              <span class="quiet">
                {pane.pane} · {shortenPath(pane.cwd)}
              </span>
              {pane.message ? <small class="quiet">{pane.message}</small> : null}
            </button>
          )}
        </For>
      </section>

      <section class="quick-grid">
        <article class="panel quick-widget">
          <header>
            <span>Sub-app servers</span>
            <strong>{app.subApps().length}</strong>
          </header>
          <For each={app.subApps().slice(0, 4)}>
            {(sub) => (
              <div class="widget-row">
                <span>{sub.title}</span>
                <strong>{sub.server}</strong>
              </div>
            )}
          </For>
        </article>

        <article class="panel quick-widget">
          <header>
            <span>Agent skills</span>
            <strong>{app.subApps().length}</strong>
          </header>
          <For each={app.subApps().slice(0, 4)}>
            {(sub) => (
              <div class="widget-row">
                <span>{sub.agent}</span>
                <strong>{sub.skills.join(" · ")}</strong>
              </div>
            )}
          </For>
        </article>

        <article class="panel quick-widget">
          <header>
            <span>Tools</span>
            <strong>{app.subApps().reduce((sum, sub) => sum + sub.tools.length, 0)}</strong>
          </header>
          <For each={app.subApps().slice(0, 4)}>
            {(sub) => (
              <div class="widget-row">
                <span>{sub.title}</span>
                <strong>{sub.tools.join(" · ")}</strong>
              </div>
            )}
          </For>
        </article>

        <article class="panel quick-widget">
          <header>
            <span>Personalization</span>
            <strong>local</strong>
          </header>
          <For each={app.subApps().slice(0, 4)}>
            {(sub) => (
              <div class="widget-row">
                <span>{sub.title}</span>
                <strong>{sub.personalization.join(" · ")}</strong>
              </div>
            )}
          </For>
        </article>
      </section>
    </div>
  );
}

export const homeSurface: SurfaceModule = {
  id: "home",
  label: "Home",
  glyph: "⌂",
  caption: "pinned ops",
  Component: HomeSurface,
  preview: (app) => ({
    title: "Pinned Ops",
    lines: [
      `${app.herd.panes().length || 6} panes`,
      `herd ${app.herd.status()}`,
      `source ${app.herd.source()}`,
      `${app.subApps().length} sub-app agents`,
    ],
  }),
  refresh: async (app) => {
    await Promise.all([
      app.runtime.loadCalendar(),
      app.runtime.loadInbox(),
      app.herd.load(),
      app.herd.loadCommands(),
    ]);
  },
};
