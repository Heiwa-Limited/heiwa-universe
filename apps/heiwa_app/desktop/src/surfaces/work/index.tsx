import { For, Index, Show, createMemo, createSignal, type JSX } from "solid-js";
import { Icon } from "../../shell/Icon";
import { useApp } from "../../state/app";
import {
  formatRecordedTime,
  runSummaryText,
  summarizeRuns,
  workRecord,
  type CollectionRows,
  type WorkSurfaceView,
  type WorkSurfaces,
} from "../../state/work-model";
import { BoundNote, CatalogNotice, DetailNotice, WorkStatusChip } from "../shared/WorkViews";
import type { SurfaceModule } from "../types";
import "./work.css";

/** Rows shown per collection before the reader asks for the rest. */
const COLLAPSED_ROWS = 20;

function str(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value : undefined;
}

/** One collection, read from the view of the snapshot that carries it. */
function Collection(props: {
  title: string;
  noun: string;
  name: string;
  view: WorkSurfaceView;
  line: (row: Record<string, unknown>) => JSX.Element;
}) {
  const [expanded, setExpanded] = createSignal(false);
  const rows = createMemo(() => Object.values((props.view.collections[props.name] ?? {}) as CollectionRows));
  const visible = () => expanded() ? rows() : rows().slice(0, COLLAPSED_ROWS);
  return <Show when={rows().length > 0 || (props.view.truncated[props.name] ?? 0) > 0}>
    <section class="work-section">
      <h3>{props.title} <small>{rows().length}</small></h3>
      <ul class="work-lines">
        <For each={visible()}>{(row) => <li>{props.line(row)}</li>}</For>
      </ul>
      <Show when={rows().length > COLLAPSED_ROWS}>
        <button class="work-more" onClick={() => setExpanded(!expanded())}>
          {expanded() ? "Show fewer" : `Show all ${rows().length} ${props.noun}`}
        </button>
      </Show>
      <BoundNote view={props.view} collection={props.name} noun={props.noun} />
    </section>
  </Show>;
}

/** Where each detail collection lives: the Work view carries all but approvals. */
const SUMMARY_COLLECTIONS: Array<[name: string, view: "work" | "agent"]> = [
  ["blockers", "work"],
  ["approvals", "agent"],
  ["actions", "work"],
  ["artifacts", "work"],
  ["tests", "work"],
  ["receipts", "work"],
  ["workspace", "work"],
];

function WorkDetail(props: { surfaces: WorkSurfaces }) {
  const app = useApp();
  const record = createMemo(() => workRecord(props.surfaces));
  const runs = createMemo(() => summarizeRuns(props.surfaces.agent));
  const conversation = createMemo(() => {
    const id = record().primaryThreadId;
    const recorded = id ? props.surfaces.work.collections.threads?.[id] : undefined;
    const session = app.sessions.threads().find((thread) => thread.thread_id === id);
    return {
      title: session?.title?.trim() || "Primary conversation",
      status: str(recorded?.status),
      updatedAt: str(recorded?.updated_at),
    };
  });
  const nothingRecorded = () => SUMMARY_COLLECTIONS.every(([name, view]) =>
    Object.keys(props.surfaces[view].collections[name] ?? {}).length === 0
    && (props.surfaces[view].truncated[name] ?? 0) === 0);

  return <article class="work-detail-body" aria-label={record().intent}>
    <header class="work-detail-head">
      <h2>{record().intent || "Untitled Work"}</h2>
      <div><WorkStatusChip status={record().status} raw={record().rawStatus} /><small>Updated {formatRecordedTime(record().updatedAt)}</small></div>
    </header>

    <section class="work-section">
      <h3>Conversation</h3>
      <p class="work-line-main">{conversation().title}</p>
      <p class="work-line-sub">
        {conversation().status ? `Last recorded turn: ${conversation().status}, ${formatRecordedTime(conversation().updatedAt)}` : "No turns recorded in this Work yet."}
        <Show when={record().relatedThreadIds.length > 0}>{` · ${record().relatedThreadIds.length} related conversation(s)`}</Show>
      </p>
    </section>

    <section class="work-section">
      <h3>Runs <small>{runs().total}</small></h3>
      <p class="work-line-sub">{runSummaryText(runs())}</p>
      <button class="work-link" onClick={() => app.navigate("workers")}>View runs in Workers <Icon name="chevron" size={14} /></button>
    </section>

    <Collection title="Blockers" noun="blockers" name="blockers" view={props.surfaces.work} line={(row) => <>
      <span class="work-line-main">{str(row.reason) ?? str(row.code) ?? "Blocked"}</span>
      <span class="work-line-sub">{formatRecordedTime(str(row.occurred_at))}</span>
    </>} />
    <Collection title="Approvals" noun="approvals" name="approvals" view={props.surfaces.agent} line={(row) => <>
      <span class="work-line-main">{str(row.tool) ?? "Action"}{str(row.risk) ? ` · ${row.risk} risk` : ""}</span>
      <span class="work-line-sub">{str(row.outcome) ?? "awaiting decision"}</span>
    </>} />
    <Collection title="Actions" noun="actions" name="actions" view={props.surfaces.work} line={(row) => <>
      <span class="work-line-main">{str(row.name) ?? "Tool call"}</span>
      <span class="work-line-sub">{str(row.status) ?? "status not recorded"}</span>
    </>} />
    <Collection title="Artifacts" noun="artifacts" name="artifacts" view={props.surfaces.work} line={(row) => <>
      <span class="work-line-main">{str(row.kind) ?? "Artifact"}</span>
      <span class="work-line-sub">{str(row.artifact_ref) ?? str(row.artifact_id) ?? ""}</span>
    </>} />
    <Collection title="Tests" noun="tests" name="tests" view={props.surfaces.work} line={(row) => <>
      <span class="work-line-main">{str(row.name) ?? "Test"}</span>
      <span class="work-line-sub">{str(row.status) ?? "status not recorded"}</span>
    </>} />
    <Collection title="Receipts" noun="receipts" name="receipts" view={props.surfaces.work} line={(row) => <>
      <span class="work-line-main">{[str(row.kind), str(row.provider), str(row.model)].filter(Boolean).join(" · ") || "Receipt"}</span>
      <span class="work-line-sub">{formatRecordedTime(str(row.occurred_at))}</span>
    </>} />
    <Collection title="Workspace" noun="workspaces" name="workspace" view={props.surfaces.work} line={(row) => <>
      <span class="work-line-main">{str(row.branch) ?? str(row.repo_root) ?? "Workspace"}</span>
      <span class="work-line-sub">{str(row.state) ?? ""}</span>
    </>} />
    <Show when={nothingRecorded()}>
      <p class="work-empty">No blockers, approvals, actions, artifacts, tests, receipts, or workspace recorded yet.</p>
    </Show>

    <details class="work-diagnostics">
      <summary>Diagnostics</summary>
      <dl>
        <dt>Work ID</dt><dd>{props.surfaces.identity.workId}</dd>
        <dt>Work revision</dt><dd>{props.surfaces.identity.workRevision}</dd>
        <dt>Projection epoch</dt><dd>{props.surfaces.identity.projectionEpoch}</dd>
        <dt>Projection revision</dt><dd>{props.surfaces.identity.projectionRevision}</dd>
        <dt>Operator cursor</dt><dd>{props.surfaces.identity.operatorCursor ?? "none"}</dd>
        <dt>Primary thread</dt><dd>{record().primaryThreadId ?? "not recorded"}</dd>
      </dl>
    </details>
  </article>;
}

function WorkSurface() {
  const app = useApp();
  const rows = () => app.work.rows();
  const detail = () => app.work.detail();

  return <div class="view catalog-view work-view">
    <header class="catalog-heading">
      <div><p>Durable goals on this Mac</p><h1>Work</h1></div>
      <div class="catalog-heading-actions">
        <button class="work-refresh" onClick={() => void Promise.all([app.work.loadCatalog(), app.work.refresh()])}>Refresh</button>
      </div>
    </header>
    <CatalogNotice state={app.work.catalog()} onRetry={() => void app.work.loadCatalog()} />
    <Show when={app.work.evidence() === "empty" && !app.work.catalog().stale}>
      <p class="catalog-empty">No Work has been created on this Mac yet.</p>
    </Show>
    <Show when={app.work.evidence() === "uncertain"}>
      <p class="catalog-empty">No readable Work to show, but the runtime reports Work this app could not list. Retry, or update Heiwa if this app is older than its runtime.</p>
    </Show>
    <div class="work-layout">
      <Show when={rows().length > 0}>
        <nav class="work-list" aria-label="Work list">
          {/* Index, not For: a snapshot replaces row objects, and re-creating the
              focused button would drop keyboard focus mid-navigation. */}
          <Index each={rows()}>{(row) => (
            <button
              class="work-list-row"
              classList={{ selected: app.work.selectedId() === row().workId }}
              aria-current={app.work.selectedId() === row().workId ? "true" : undefined}
              onClick={() => void app.work.select(row().workId)}
            >
              <span class="work-list-text"><strong>{row().intent || "Untitled Work"}</strong><small>Updated {formatRecordedTime(row().updatedAt)}</small></span>
              <WorkStatusChip status={row().status} raw={row().rawStatus} />
            </button>
          )}</Index>
        </nav>
      </Show>
      <section class="work-detail" aria-live="polite">
        <Show when={detail().workId} fallback={<Show when={rows().length > 0}><p class="work-empty">Select a Work to see its details.</p></Show>}>
          <DetailNotice state={detail()} onRetry={() => void app.work.refresh()} />
          <Show when={detail().surfaces} keyed>{(surfaces) => <WorkDetail surfaces={surfaces} />}</Show>
        </Show>
      </section>
    </div>
  </div>;
}

export const workSurface: SurfaceModule = {
  id: "work",
  label: "Work",
  glyph: "",
  caption: "Viewing Work",
  Component: WorkSurface,
  preview: (app) => {
    const catalog = app.work.catalog().catalog;
    return { title: "Work", lines: [catalog ? `${catalog.total} Work` : "Not loaded"] };
  },
  refresh: (app) => Promise.all([app.work.loadCatalog(), app.work.refresh()]).then(() => undefined),
};
