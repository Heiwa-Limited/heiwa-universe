import { For, Show } from "solid-js";
import { useApp } from "../../state/app";
import { Icon } from "../../shell/Icon";
import { localIsoDate } from "../../lib/format";
import { TodayBriefing } from "./TodayBriefing";
import type { SurfaceModule } from "../types";
import "./home.css";

function HomeSurface() {
  const app = useApp();
  const recent = () => app.sessions.threads()
    .filter((thread) => !thread.archived && Boolean(thread.title?.trim()) && (thread.turn_count ?? 1) > 0)
    .slice(0, 6);
  const projects = () => app.sessions.projects().filter((project) => !project.archived);
  const hasToday = () => app.runtime.calendarEvents().some((event) => event.date === localIsoDate())
    || app.runtime.mail().some((message) => message.unread);
  const projectTitle = (id: string | null | undefined) =>
    app.sessions.projects().find((project) => project.project_id === id)?.title;
  const open = (id: string) => void app.sessions.select(id)
    .then(() => app.navigate("ai")).catch(() => undefined);

  return (
    <div class="view home-view">
      <section class="home-intro">
        <div class="home-eyebrow"><span>Your space</span><span>{new Intl.DateTimeFormat(undefined, { weekday: "long", month: "long", day: "numeric" }).format(new Date())}</span></div>
        <h1>What’s on your mind?</h1>
        <p>Pick up a conversation, or start something new.</p>
      </section>
      <section class="home-recent">
        <header><h2>Recent sessions</h2><button onClick={() => app.navigate("sessions")}>All sessions <Icon name="chevron" size={14} /></button></header>
        <Show when={app.sessions.loading()}><p class="home-empty">Loading your sessions…</p></Show>
        <Show when={!app.sessions.loading() && recent().length === 0}>
          <div class="home-empty"><Icon name="sessions" size={24} /><div><strong>A place for your next idea.</strong><p>Start with the chatbar below. Your conversations will be here when you return.</p></div></div>
        </Show>
        <For each={recent()}>{(thread) => (
          <button class="home-session-row" onClick={() => open(thread.thread_id)}>
            <span class="home-row-icon"><Icon name="sessions" size={18} /></span>
            <span class="home-row-text"><strong>{thread.title}</strong><small>{projectTitle(thread.project_id) ?? "Standalone session"}</small></span>
            <small>{thread.latest_status ?? "Conversation"}</small><Icon name="chevron" size={16} />
          </button>
        )}</For>
      </section>
      <Show when={hasToday()}><TodayBriefing /></Show>
      <section class="home-projects">
        <header><h2>Projects</h2><span>{projects().length || ""}</span></header>
        <Show when={projects().length === 0}>
          <p class="home-project-empty">Keep related conversations together. Create a project from the sidebar.</p>
        </Show>
        <For each={projects()}>{(project) => (
          <button class="home-project-row" onClick={() => { app.selectProject(project.project_id); app.navigate("projects"); }}>
            <Icon name="folder" size={19} /><span>{project.title}</span>
            <small>{app.sessions.threads().filter((thread) => thread.project_id === project.project_id && !thread.archived).length} sessions</small>
            <Icon name="chevron" size={16} />
          </button>
        )}</For>
      </section>
    </div>
  );
}

export const homeSurface: SurfaceModule = {
  id: "home", label: "Home", glyph: "", caption: "Viewing Home", Component: HomeSurface,
  preview: () => ({ title: "Home", lines: ["Recent work"] }),
  refresh: (app) => Promise.all([app.runtime.loadCalendar(), app.runtime.loadMail()]).then(() => undefined),
};
