import { For, Show } from "solid-js";
import { cssToken, shortenPath } from "../../lib/format";
import { TodayBriefing } from "./TodayBriefing";
import { MachinePerspective } from "./MachinePerspective";
import { useApp } from "../../state/app";
import type { SurfaceModule } from "../types";
import "./home.css";

function HomeSurface() {
  const app = useApp();
  const pinned = () => app.herd.panes().slice(0, 8);

  return (
    <div class="view home-view">
      <section class="home-command">
        <div>
          <h1>Heiwa Ops</h1>
          <p class="muted">
            {pinned().length} live panes · herd {app.herd.status()} via {app.herd.source()} ·{" "}
            {app.subApps().length} available capabilities
          </p>
        </div>
        <button class="btn-primary" onClick={() => app.navigate("windows")}>
          Open Windows
        </button>
      </section>

      {/*
        First thing on the page, because it is the first thing a user wants:
        what today holds, from data already on this machine.
      */}
      <TodayBriefing />

      <MachinePerspective />

      <section class="pinned-pane-board" aria-label="Live terminal and ops panes">
        <Show
          when={pinned().length > 0}
          fallback={
            <div class="panel home-empty-panes">
              <strong>No live panes.</strong>
              <span class="quiet">Terminal work appears here only after herdr reports it.</span>
            </div>
          }
        >
          <For each={pinned()}>
            {(pane, index) => (
              <button
                class="pinned-pane"
                classList={{ "primary-pane": index() === 0 }}
                onClick={() => {
                  app.herd.select(pane.pane);
                  app.navigate("windows");
                  void app.herd.loadPaneText();
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
        </Show>
      </section>

      <section class="quick-grid">
        <article class="panel quick-widget">
          <header>
            <span>Available surfaces</span>
            <strong>{app.subApps().length}</strong>
          </header>
          <For each={app.subApps().slice(0, 4)}>
            {(sub) => (
              <div class="widget-row">
                <span>{sub.title}</span>
                <strong>{sub.state}</strong>
              </div>
            )}
          </For>
        </article>

        <article class="panel quick-widget">
          <header>
            <span>Capability profiles</span>
            <strong>{app.subApps().length}</strong>
          </header>
          <For each={app.subApps().slice(0, 4)}>
            {(sub) => (
              <div class="widget-row">
                <span>{sub.title}</span>
                <strong>{sub.skills.join(" · ")}</strong>
              </div>
            )}
          </For>
        </article>

        <article class="panel quick-widget">
          <header>
            <span>Tool policies</span>
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
            <span>Local policy defaults</span>
            <strong>built in</strong>
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
  caption: "local ops",
  Component: HomeSurface,
  preview: (app) => ({
    title: "Local Ops",
    lines: [
      `${app.herd.panes().length} live panes`,
      `herd ${app.herd.status()}`,
      `source ${app.herd.source()}`,
      `${app.subApps().length} available capabilities`,
    ],
  }),
  refresh: async (app) => {
    await Promise.all([
      app.runtime.loadCalendar(),
      app.runtime.loadInbox(),
      // The briefing reads mail too, so Home has to load it — otherwise the
      // unread count is only correct after the user visits Mail.
      app.runtime.loadMail(),
      app.runtime.loadHealth(),
      app.herd.load(),
      app.herd.loadCommands(),
    ]);
  },
};
