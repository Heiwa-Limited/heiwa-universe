import { For, Index, Show } from "solid-js";
import { useApp } from "../../state/app";
import { Icon } from "../../shell/Icon";
import { localIsoDate } from "../../lib/format";
import { normalizeEvents, occursOn } from "../../state/calendar-model";
import {
  blockerCount,
  formatRecordedTime,
  homeOrder,
  pendingApprovals,
  runAttention,
  summarizeRuns,
} from "../../state/work-model";
import type { WorkRow } from "../../state/work";
import { CatalogNotice, WorkDetailProblem, WorkStatusChip } from "../shared/WorkViews";
import { TodayBriefing } from "./TodayBriefing";
import type { SurfaceModule } from "../types";
import "./home.css";

/** Work rows Home shows, and the most Work projections one Home refresh reads. */
const HOME_WORK_ROWS = 3;

function HomeSurface() {
  const app = useApp();
  const recent = () => app.sessions.threads()
    .filter((thread) => !thread.archived && Boolean(thread.title?.trim()) && (thread.turn_count ?? 1) > 0)
    .slice(0, 6);
  const projects = () => app.sessions.projects().filter((project) => !project.archived);
  const hasToday = () => normalizeEvents(app.runtime.calendarEvents()).some((event) => occursOn(event, localIsoDate()))
    || app.runtime.mail().some((message) => message.unread);
  const projectTitle = (id: string | null | undefined) =>
    app.sessions.projects().find((project) => project.project_id === id)?.title;
  const open = (id: string) => void app.sessions.select(id)
    .then(() => app.navigate("ai")).catch(() => undefined);
  // The shared order and bound the Home refresh prefetches projections for.
  const work = () => homeOrder(app.work.rows()).slice(0, HOME_WORK_ROWS);
  // Hidden only when the catalog proves there is no Work, or has not loaded.
  const showWork = () => work().length > 0
    || app.work.catalog().status === "error"
    || app.work.evidence() === "uncertain";
  /** What the Work's Home projection says needs attention, when Heiwa has it. */
  const facts = (row: WorkRow): string => {
    const view = row.snapshot?.home;
    if (!view) return "";
    const parts = runAttention(summarizeRuns(view));
    const blockers = blockerCount(view);
    const approvals = pendingApprovals(view);
    if (blockers > 0) parts.push(`${blockers} blocker${blockers === 1 ? "" : "s"}`);
    if (approvals > 0) parts.push(`${approvals} awaiting approval`);
    return parts.length ? ` · ${parts.join(" · ")}` : "";
  };
  const openWork = (id: string) => {
    app.navigate("work");
    void app.work.select(id);
  };

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
      <Show when={showWork()}>
        <section class="home-recent home-work">
          <header><h2>Work</h2><button onClick={() => app.navigate("work")}>All Work <Icon name="chevron" size={14} /></button></header>
          <CatalogNotice state={app.work.catalog()} onRetry={() => void app.work.loadCatalog({ prefetch: HOME_WORK_ROWS })} />
          <Show when={app.work.evidence() === "uncertain"}>
            <p class="home-project-empty">No readable Work to show, but the runtime reports Work this app could not list.</p>
          </Show>
          <Index each={work()}>{(row) => <>
            <button class="home-session-row" onClick={() => openWork(row().workId)}>
              <span class="home-row-icon"><Icon name="work" size={18} /></span>
              <span class="home-row-text"><strong>{row().intent || "Untitled Work"}</strong><small>Updated {formatRecordedTime(row().updatedAt)}{facts(row())}</small></span>
              <WorkStatusChip status={row().status} raw={row().rawStatus} /><Icon name="chevron" size={16} />
            </button>
            <WorkDetailProblem row={row()} onRetry={() => void app.work.retry(row().workId)} />
          </>}</Index>
        </section>
      </Show>
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
  // The briefing answers "today" from the calendar, so arriving also asks the
  // runtime to re-read it when its copy is more than a few minutes old.
  refresh: (app) => Promise.all([
    app.runtime.syncCalendar({ maxAgeSeconds: 300 }),
    app.runtime.loadCalendar(),
    app.runtime.loadMail(),
    app.runtime.syncMail({ background: true, staleSeconds: 300 }),
    app.work.loadCatalog({ prefetch: HOME_WORK_ROWS }),
  ]).then(() => undefined),
};
