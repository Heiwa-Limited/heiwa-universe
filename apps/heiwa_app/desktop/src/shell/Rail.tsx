import { For, Show, createMemo, createSignal } from "solid-js";
import { useApp } from "../state/app";
import type { SurfaceModule } from "../surfaces/types";
import { Icon } from "./Icon";
import { CreateProjectControl, ProjectActions, SessionActions } from "./SessionControls";

const NAV: Array<{
  id: "home" | "sessions" | "calendar" | "mail";
  label: string;
  icon: "home" | "sessions" | "calendar" | "mail";
}> = [
  { id: "home", label: "Home", icon: "home" },
  { id: "sessions", label: "All sessions", icon: "sessions" },
  { id: "calendar", label: "Calendar", icon: "calendar" },
  { id: "mail", label: "Mail", icon: "mail" },
];

export function Rail(props: { onNavigate: (surface: SurfaceModule) => void; onResources?: () => void }) {
  const app = useApp();
  const [query, setQuery] = createSignal("");
  const filter = (value: string | null | undefined) =>
    !query() || (value ?? "").toLowerCase().includes(query().toLowerCase());
  const project = (id: string | null | undefined) =>
    app.sessions.projects().find((item) => item.project_id === id);
  const standalone = createMemo(() => app.sessions.threads().filter((thread) =>
    !thread.archived
    && (!thread.project_id || !project(thread.project_id) || project(thread.project_id)?.archived)
    && filter(thread.title),
  ));
  const projects = createMemo(() => app.sessions.projects().filter((item) =>
    !item.archived
    && (filter(item.title) || app.sessions.threads().some((thread) =>
      thread.project_id === item.project_id && filter(thread.title),
    )),
  ));
  const open = (id: string) => void app.sessions.select(id)
    .then(() => app.navigate("ai"))
    .catch(() => undefined);

  return <aside class="workspace-sidebar" aria-label="Workspace">
    <div class="sidebar-traffic" data-tauri-drag-region aria-hidden="true" />
    <div class="sidebar-brand">
      <span class="brand-mark"><Icon name="heiwa" size={26}/></span>
      <strong>heiwa</strong>
    </div>
    <nav class="sidebar-navlist">
      <For each={NAV}>{(item) => <button classList={{ active: app.view() === item.id }} onClick={() => app.navigate(item.id)}>
        <Icon name={item.icon} />{item.label}
      </button>}</For>
    </nav>
    <label class="sidebar-search">
      <Icon name="search" size={16} />
      <input
        value={query()}
        onInput={(event) => setQuery(event.currentTarget.value)}
        placeholder="Search sessions"
        aria-label="Search sessions"
      />
    </label>
    <div class="sidebar-label"><span>Sessions</span></div>
    <Show when={app.sessions.loading()}><p class="sidebar-note">Loading sessions…</p></Show>
    <Show when={app.sessions.error()}><p class="sidebar-note sidebar-error">
      {app.sessions.error()} <button onClick={() => void app.sessions.load()}>Retry</button>
    </p></Show>
    <Show when={app.sessions.truncated()}><p class="sidebar-note">Showing a bounded session list.</p></Show>
    <div class="sidebar-session-list">
      <For each={standalone()}>{(thread) => <div class="sidebar-session-line">
        <button
          class="sidebar-session"
          classList={{ selected: app.sessions.selectedId() === thread.thread_id }}
          onClick={() => open(thread.thread_id)}
        ><span>{thread.title?.trim() || "Untitled session"}</span></button>
        <SessionActions thread={thread} />
      </div>}</For>
    </div>
    <div class="sidebar-label">
      <span>Projects</span>
      <CreateProjectControl ariaLabel="New project"><Icon name="plus" size={16}/></CreateProjectControl>
    </div>
    <div class="sidebar-project-list">
      <For each={projects()}>{(item) => <section>
        <header>
          <button onClick={() => { app.selectProject(item.project_id); app.navigate("projects"); }}>{item.title}</button>
          <ProjectActions project={item} />
        </header>
        <For each={app.sessions.threads().filter((thread) =>
          !thread.archived && thread.project_id === item.project_id && filter(thread.title),
        )}>{(thread) => <div class="sidebar-session-line">
          <button
            class="sidebar-session"
            classList={{ selected: app.sessions.selectedId() === thread.thread_id }}
            onClick={() => open(thread.thread_id)}
          >{thread.title?.trim() || "Untitled session"}</button>
          <SessionActions thread={thread} />
        </div>}</For>
      </section>}</For>
    </div>
    <Show when={app.sessions.threads().some((thread) => thread.archived) || app.sessions.projects().some((item) => item.archived)}>
      <details class="sidebar-archived">
        <summary>Archived</summary>
        <For each={app.sessions.threads().filter((thread) => thread.archived)}>{(thread) => <div class="sidebar-archived-row">
          <span>{thread.title || "Untitled session"}</span><SessionActions thread={thread} />
        </div>}</For>
        <For each={app.sessions.projects().filter((item) => item.archived)}>{(item) => <div class="sidebar-archived-row">
          <span>{item.title}</span><ProjectActions project={item} />
        </div>}</For>
      </details>
    </Show>
    <div class="sidebar-fill" />
    <Show when={props.onResources}><button class="sidebar-resources" onClick={() => props.onResources?.()}>
      Your resources <Icon name="chevron" size={15} />
    </button></Show>
  </aside>;
}
