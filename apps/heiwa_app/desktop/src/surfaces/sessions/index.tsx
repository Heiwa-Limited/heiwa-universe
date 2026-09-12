import { For, Show, createMemo, createSignal } from "solid-js";
import { Icon } from "../../shell/Icon";
import { ProjectActions, SessionActions } from "../../shell/SessionControls";
import { useApp } from "../../state/app";
import type { SurfaceModule } from "../types";
import "./sessions.css";

function open(app: ReturnType<typeof useApp>, id: string) {
  void app.sessions.select(id).then(() => app.navigate("ai")).catch(() => undefined);
}

function AllSessions() {
  const app = useApp();
  const [query, setQuery] = createSignal("");
  const projectName = (id: string | null | undefined) => app.sessions.projects().find((project) => project.project_id === id)?.title;
  const rows = createMemo(() => app.sessions.threads().filter((thread) =>
    !thread.archived
    && `${thread.title ?? ""} ${projectName(thread.project_id) ?? ""}`.toLowerCase().includes(query().toLowerCase()),
  ));

  return <div class="view catalog-view">
    <header class="catalog-heading">
      <div><p>Every conversation</p><h1>All sessions</h1></div>
      <div class="catalog-heading-actions">
        <label><Icon name="search" size={16}/><input value={query()} onInput={(event) => setQuery(event.currentTarget.value)} placeholder="Filter sessions" aria-label="Filter sessions"/></label>
      </div>
    </header>
    <Show when={app.sessions.loading()}><p class="catalog-empty">Loading sessions…</p></Show>
    <Show when={!app.sessions.loading() && rows().length === 0}><p class="catalog-empty">No matching sessions.</p></Show>
    <For each={rows()}>{(thread) => <div class="catalog-row">
      <button class="catalog-row-main" onClick={() => open(app, thread.thread_id)}>
        <span class="catalog-icon"><Icon name="sessions"/></span>
        <span><strong>{thread.title?.trim() || "Untitled session"}</strong><small>{projectName(thread.project_id) ?? "Standalone session"}</small></span>
        <small>{thread.latest_status ?? "Conversation"}</small>
        <Icon name="chevron" size={16}/>
      </button>
      <SessionActions thread={thread} />
    </div>}</For>
  </div>;
}

function ProjectDetail() {
  const app = useApp();
  const project = () => app.sessions.projects().find((item) => item.project_id === app.selectedProjectId());
  const rows = () => app.sessions.threads().filter((thread) => !thread.archived && thread.project_id === project()?.project_id);

  return <div class="view catalog-view">
    <Show when={project()} fallback={<p class="catalog-empty">Select a project from the sidebar.</p>} keyed>{(selected) => <>
      <header class="catalog-heading">
        <div><p>Project</p><h1>{selected.title}</h1></div>
        <div class="catalog-heading-actions"><span>{rows().length} sessions</span><ProjectActions project={selected}/></div>
      </header>
      <Show when={rows().length === 0}><p class="catalog-empty">No sessions in this project.</p></Show>
      <For each={rows()}>{(thread) => <div class="catalog-row">
        <button class="catalog-row-main" onClick={() => open(app, thread.thread_id)}>
          <span class="catalog-icon"><Icon name="sessions"/></span>
          <span><strong>{thread.title?.trim() || "Untitled session"}</strong><small>Project session</small></span>
          <Icon name="chevron" size={16}/>
        </button>
        <SessionActions thread={thread}/>
      </div>}</For>
    </>}</Show>
  </div>;
}

export const sessionsSurface: SurfaceModule = {
  id: "sessions",
  label: "All sessions",
  glyph: "",
  caption: "Viewing all sessions",
  Component: AllSessions,
  preview: () => ({ title: "All sessions", lines: ["Catalog"] }),
};

export const projectsSurface: SurfaceModule = {
  id: "projects",
  label: "Project",
  glyph: "",
  caption: "Viewing project",
  Component: ProjectDetail,
  preview: () => ({ title: "Project", lines: ["Catalog"] }),
};
