/**
 * Work catalog and Work-surface payloads, validated at the edge.
 *
 * The runtime builds Home, Work, and Agent from one snapshot. This module
 * refuses any response that does not prove that — three named views, one
 * identity, the requested Work — so presentation never renders a payload it
 * cannot vouch for. Epochs and cursors stay opaque strings.
 */

export const SURFACE_NAMES = ["home", "work", "agent"] as const;
export type SurfaceName = (typeof SURFACE_NAMES)[number];

export type WorkStatus = "active" | "blocked" | "complete" | "failed" | "cancelled" | "unrecognized";

export type WorkCatalogRow = {
  workId: string;
  intent: string;
  status: WorkStatus;
  rawStatus: string;
  revision: number;
  primaryThreadId: string;
  relatedThreadIds: string[];
  updatedAt: string;
};

export type WorkCatalog = {
  rows: WorkCatalogRow[];
  /** Every Work on the installation, including rows the bound left out. */
  total: number;
  /** Rows the runtime omitted at its bound. */
  truncated: number;
  /** Work events the runtime could not fold. */
  skippedEvents: number;
  /** Rows in this response this build could not read. */
  unreadableRows: number;
};

export type SurfaceIdentity = {
  workId: string;
  workRevision: number;
  projectionEpoch: string;
  projectionRevision: number;
  operatorCursor: string | null;
};

export type CollectionRows = Record<string, Record<string, unknown>>;

export type WorkSurfaceView = {
  surface: SurfaceName;
  identity: SurfaceIdentity;
  collections: Record<string, CollectionRows>;
  truncated: Record<string, number>;
};

export type WorkSurfaces = {
  identity: SurfaceIdentity;
  home: WorkSurfaceView;
  work: WorkSurfaceView;
  agent: WorkSurfaceView;
};

/** A response this build cannot interpret. It must never render as Work. */
export class IncompatibleWorkPayload extends Error {
  constructor(detail: string) {
    super(`The runtime returned Work data this app cannot read (${detail}).`);
    this.name = "IncompatibleWorkPayload";
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isCount(value: unknown): value is number {
  return typeof value === "number" && Number.isInteger(value) && value >= 0;
}

function text(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}

const WORK_STATUSES: ReadonlySet<string> = new Set(["active", "blocked", "complete", "failed", "cancelled"]);

function workStatus(value: string): WorkStatus {
  return WORK_STATUSES.has(value) ? (value as WorkStatus) : "unrecognized";
}

export function parseWorkCatalog(payload: unknown): WorkCatalog {
  if (!isRecord(payload) || payload.ok !== true || !isRecord(payload.data)) {
    throw new IncompatibleWorkPayload("catalog envelope");
  }
  const data = payload.data;
  if (!Array.isArray(data.work) || !isCount(data.total) || !isCount(data.truncated) || !isCount(data.skipped_events)) {
    throw new IncompatibleWorkPayload("catalog fields");
  }
  const rows: WorkCatalogRow[] = [];
  let unreadableRows = 0;
  for (const row of data.work) {
    const workId = isRecord(row) ? text(row.work_id) : undefined;
    const intent = isRecord(row) ? text(row.intent) : undefined;
    const status = isRecord(row) ? text(row.status) : undefined;
    const primaryThreadId = isRecord(row) ? text(row.primary_thread_id) : undefined;
    const updatedAt = isRecord(row) ? text(row.updated_at) : undefined;
    if (!isRecord(row) || !workId || intent === undefined || !status || !primaryThreadId || !updatedAt || !isCount(row.revision)) {
      unreadableRows += 1;
      continue;
    }
    rows.push({
      workId,
      intent,
      status: workStatus(status),
      rawStatus: status,
      revision: row.revision,
      primaryThreadId,
      relatedThreadIds: Array.isArray(row.related_thread_ids)
        ? row.related_thread_ids.filter((id): id is string => typeof id === "string")
        : [],
      updatedAt,
    });
  }
  return {
    rows,
    total: data.total,
    truncated: data.truncated,
    skippedEvents: data.skipped_events,
    unreadableRows,
  };
}

function parseIdentity(value: unknown): SurfaceIdentity {
  if (!isRecord(value)) throw new IncompatibleWorkPayload("surface identity");
  const workId = text(value.work_id);
  const projectionEpoch = text(value.projection_epoch);
  const cursor = value.operator_cursor;
  if (
    !workId
    || !projectionEpoch
    || !isCount(value.work_revision)
    || !isCount(value.projection_revision)
    || !(cursor === null || typeof cursor === "string")
  ) {
    throw new IncompatibleWorkPayload("surface identity fields");
  }
  return {
    workId,
    workRevision: value.work_revision,
    projectionEpoch,
    projectionRevision: value.projection_revision,
    operatorCursor: cursor,
  };
}

function sameIdentity(left: SurfaceIdentity, right: SurfaceIdentity): boolean {
  return left.workId === right.workId
    && left.workRevision === right.workRevision
    && left.projectionEpoch === right.projectionEpoch
    && left.projectionRevision === right.projectionRevision
    && left.operatorCursor === right.operatorCursor;
}

function parseView(value: unknown): WorkSurfaceView {
  if (!isRecord(value)) throw new IncompatibleWorkPayload("surface view");
  const surface = text(value.surface);
  if (!surface || !(SURFACE_NAMES as readonly string[]).includes(surface)) {
    throw new IncompatibleWorkPayload("surface name");
  }
  if (!isRecord(value.collections)) throw new IncompatibleWorkPayload("surface collections");
  const collections: Record<string, CollectionRows> = {};
  for (const [name, rows] of Object.entries(value.collections)) {
    if (!isRecord(rows) || !Object.values(rows).every(isRecord)) {
      throw new IncompatibleWorkPayload(`collection ${name}`);
    }
    collections[name] = rows as CollectionRows;
  }
  const bounds = value.truncated_collections ?? {};
  if (!isRecord(bounds) || !Object.values(bounds).every(isCount)) {
    throw new IncompatibleWorkPayload("collection bounds");
  }
  return {
    surface: surface as SurfaceName,
    identity: parseIdentity(value.identity),
    collections,
    truncated: bounds as Record<string, number>,
  };
}

/**
 * Accept a surfaces response only when it is exactly Home, Work, and Agent of
 * one snapshot of the Work that was asked for.
 */
export function parseWorkSurfaces(payload: unknown, requestedWorkId: string): WorkSurfaces {
  if (!isRecord(payload) || payload.ok !== true || !isRecord(payload.data) || !Array.isArray(payload.data.surfaces)) {
    throw new IncompatibleWorkPayload("surfaces envelope");
  }
  const views = payload.data.surfaces.map(parseView);
  const byName = new Map(views.map((view) => [view.surface, view]));
  if (views.length !== SURFACE_NAMES.length || byName.size !== SURFACE_NAMES.length) {
    throw new IncompatibleWorkPayload("surface set");
  }
  const home = byName.get("home")!;
  const work = byName.get("work")!;
  const agent = byName.get("agent")!;
  const identity = home.identity;
  if (!sameIdentity(identity, work.identity) || !sameIdentity(identity, agent.identity)) {
    throw new IncompatibleWorkPayload("surfaces disagree on identity");
  }
  if (identity.workId !== requestedWorkId) {
    throw new IncompatibleWorkPayload("response names another Work");
  }
  const row = work.collections.work?.[requestedWorkId];
  if (!row || typeof row.status !== "string" || typeof row.intent !== "string") {
    throw new IncompatibleWorkPayload("Work record");
  }
  return { identity, home, work, agent };
}

export type WorkRecord = {
  intent: string;
  status: WorkStatus;
  rawStatus: string;
  primaryThreadId?: string;
  relatedThreadIds: string[];
  updatedAt?: string;
};

export function workRecord(surfaces: WorkSurfaces): WorkRecord {
  const row = surfaces.work.collections.work[surfaces.identity.workId];
  const rawStatus = String(row.status);
  return {
    intent: String(row.intent),
    status: workStatus(rawStatus),
    rawStatus,
    primaryThreadId: text(row.primary_thread_id),
    relatedThreadIds: Array.isArray(row.related_thread_ids)
      ? row.related_thread_ids.filter((id): id is string => typeof id === "string")
      : [],
    updatedAt: text(row.updated_at),
  };
}

export function statusLabel(status: WorkStatus, raw?: string): string {
  switch (status) {
    case "active": return "Active";
    case "blocked": return "Blocked";
    case "complete": return "Complete";
    case "failed": return "Failed";
    case "cancelled": return "Cancelled";
    default: return raw ? `Unrecognized status “${raw}”` : "Unrecognized status";
  }
}

export function isUnfinished(status: WorkStatus): boolean {
  return status === "active" || status === "blocked";
}

// ── Runs ────────────────────────────────────────────────────────────────────

export type WorkerState = "starting" | "live" | "stale" | "exited" | "failed" | "revoked" | "unrecognized";
export type ObservedProcess = "alive" | "gone" | "unknown";

export type SupervisionLoss = {
  process: ObservedProcess;
  /** The observation as recorded, if it was not one this build knows. */
  rawProcess?: string;
  pid: number | null;
  recordedAt?: string;
};

export type WorkRun = {
  /** Thread + worker + invocation: repeated launches of one worker stay separate. */
  key: string;
  runId: string;
  workerId: string;
  threadId: string;
  workerState: WorkerState;
  rawState: string;
  provider?: string;
  startedAt?: string;
  endedAt?: string;
  exitCode: number | null;
  failureCode?: string;
  pid: number | null;
  supervision?: SupervisionLoss;
  paneState?: string;
};

const WORKER_STATES: ReadonlySet<string> = new Set(["starting", "live", "stale", "exited", "failed", "revoked"]);

function nullableInteger(value: unknown): number | null {
  return typeof value === "number" && Number.isInteger(value) ? value : null;
}

function parseSupervision(value: unknown): SupervisionLoss | undefined {
  if (value === null || value === undefined) return undefined;
  if (!isRecord(value)) return { process: "unknown", pid: null };
  const raw = text(value.process);
  const known = raw === "alive" || raw === "gone" || raw === "unknown";
  return {
    process: known ? raw : "unknown",
    rawProcess: known ? undefined : raw,
    pid: nullableInteger(value.pid),
    recordedAt: text(value.recorded_at),
  };
}

/** Runs from the Agent view, newest first. Rows without identity are counted, not guessed. */
export function runsFrom(view: WorkSurfaceView): { runs: WorkRun[]; unreadable: number } {
  const rows = view.collections.runs ?? {};
  const runs: WorkRun[] = [];
  let unreadable = 0;
  for (const row of Object.values(rows)) {
    const runId = text(row.run_id);
    const workerId = text(row.worker_id);
    const rawState = text(row.worker_state);
    if (!runId || !workerId || !rawState) {
      unreadable += 1;
      continue;
    }
    const threadId = text(row.thread_id) ?? "";
    runs.push({
      key: `${threadId}/${workerId}/${runId}`,
      runId,
      workerId,
      threadId,
      workerState: WORKER_STATES.has(rawState) ? (rawState as WorkerState) : "unrecognized",
      rawState,
      provider: text(row.provider),
      startedAt: text(row.started_at),
      endedAt: text(row.ended_at),
      exitCode: nullableInteger(row.exit_code),
      failureCode: text(row.failure_code),
      pid: nullableInteger(row.pid),
      supervision: parseSupervision(row.supervision),
      paneState: text(row.pane_state),
    });
  }
  runs.sort((left, right) =>
    (right.startedAt ?? "").localeCompare(left.startedAt ?? "") || left.key.localeCompare(right.key));
  return { runs, unreadable };
}

export type RunTone = "active" | "done" | "attention" | "failed" | "unknown";

export type RunDescription = {
  label: string;
  tone: RunTone;
  detail?: string;
  /** Recorded loss of supervision, kept even after a later exit. */
  supervision?: { headline: string; observation: string };
};

export function formatRecordedTime(value: string | undefined): string {
  if (!value) return "an unrecorded time";
  const parsed = Date.parse(value);
  if (Number.isNaN(parsed)) return value;
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(new Date(parsed));
}

export function describeSupervision(loss: SupervisionLoss): { headline: string; observation: string } {
  const when = formatRecordedTime(loss.recordedAt);
  switch (loss.process) {
    case "alive":
      return {
        headline: "Supervision lost",
        observation: `The process was still running when recovery checked at ${when}. Heiwa did not stop it and cannot vouch for it since.`,
      };
    case "gone":
      return {
        headline: "Supervision lost",
        observation: `The process was no longer running when recovery checked at ${when}. No exit was recorded.`,
      };
    default:
      return {
        headline: "Supervision lost",
        observation: loss.rawProcess
          ? `Recovery recorded an observation this app does not recognize (“${loss.rawProcess}”) at ${when}.`
          : `Recovery could not tell whether the process was running when it checked at ${when}.`,
      };
  }
}

export function describeRun(run: WorkRun): RunDescription {
  const supervision = run.supervision ? describeSupervision(run.supervision) : undefined;
  switch (run.workerState) {
    case "starting":
      return { label: "Starting", tone: "active", detail: "Launched; not yet confirmed running.", supervision };
    case "live":
      return { label: "Running", tone: "active", detail: "Its supervising process recorded a heartbeat.", supervision };
    case "stale":
      return { label: "Supervision lost", tone: "attention", supervision };
    case "exited":
      if (run.exitCode === 0) return { label: "Finished", tone: "done", supervision };
      if (run.exitCode !== null) return { label: `Exited with code ${run.exitCode}`, tone: "failed", supervision };
      return { label: "Ended without an exit code", tone: "failed", supervision };
    case "failed":
      return { label: run.failureCode ? `Failed: ${run.failureCode}` : "Failed", tone: "failed", supervision };
    case "revoked":
      return { label: "Revoked", tone: "failed", supervision };
    default:
      return { label: `Unrecognized state “${run.rawState}”`, tone: "unknown", supervision };
  }
}

// ── Catalog evidence ───────────────────────────────────────────────────────

/**
 * What the catalog proves about the installation. `empty` needs positive
 * evidence: no rows, and nothing omitted, skipped, or unreadable. Anything
 * else with no readable rows is `uncertain`, never "no Work".
 */
export type CatalogEvidence = "unknown" | "empty" | "listed" | "uncertain";

export function catalogEvidence(catalog: WorkCatalog | undefined): CatalogEvidence {
  if (!catalog) return "unknown";
  if (catalog.rows.length > 0) return "listed";
  const nothingMissing = catalog.total === 0
    && catalog.truncated === 0
    && catalog.skippedEvents === 0
    && catalog.unreadableRows === 0;
  return nothingMissing ? "empty" : "uncertain";
}

/** Home's order: unfinished Work first, each group keeping catalog order. */
export function homeOrder<T extends { status: WorkStatus }>(rows: readonly T[]): T[] {
  return [...rows.filter((row) => isUnfinished(row.status)), ...rows.filter((row) => !isUnfinished(row.status))];
}

// ── Run and projection summaries ───────────────────────────────────────────

export type RunSummary = {
  total: number;
  lostSupervision: number;
  failed: number;
  unrecognized: number;
  active: number;
  finished: number;
  /** Run records this build could not read. */
  unreadable: number;
  /** Runs the runtime left out at its bound. */
  omitted: number;
};

/** Counts from the same classification the run list shows. */
export function summarizeRuns(view: WorkSurfaceView): RunSummary {
  const { runs, unreadable } = runsFrom(view);
  const summary: RunSummary = {
    total: runs.length,
    lostSupervision: 0,
    failed: 0,
    unrecognized: 0,
    active: 0,
    finished: 0,
    unreadable,
    omitted: view.truncated.runs ?? 0,
  };
  for (const run of runs) {
    switch (describeRun(run).tone) {
      case "attention": summary.lostSupervision += 1; break;
      case "failed": summary.failed += 1; break;
      case "unknown": summary.unrecognized += 1; break;
      case "active": summary.active += 1; break;
      default: summary.finished += 1;
    }
  }
  return summary;
}

/** The parts of a run summary that need someone's attention, e.g. "1 failed". */
export function runAttention(summary: RunSummary): string[] {
  const parts: string[] = [];
  if (summary.lostSupervision > 0) parts.push(`${summary.lostSupervision} lost supervision`);
  if (summary.failed > 0) parts.push(`${summary.failed} failed`);
  if (summary.unrecognized > 0) parts.push(`${summary.unrecognized} in an unrecognized state`);
  return parts;
}

/** One sentence about runs that never claims more than the snapshot shows. */
export function runSummaryText(summary: RunSummary): string {
  const attention = runAttention(summary);
  const incomplete = summary.unreadable > 0 || summary.omitted > 0;
  if (attention.length > 0) {
    return `${attention.join(" · ")}.${incomplete ? " Some run records are not shown." : ""}`;
  }
  if (summary.total === 0 && !incomplete) return "No runs recorded.";
  if (incomplete) return "No readable run needs attention, but some run records are not shown.";
  return "None need attention.";
}

/** Approvals the Home projection shows without a recorded outcome. */
export function pendingApprovals(view: WorkSurfaceView): number {
  return Object.values(view.collections.approvals ?? {}).filter((row) => typeof row.outcome !== "string").length;
}

export function blockerCount(view: WorkSurfaceView): number {
  return Object.keys(view.collections.blockers ?? {}).length + (view.truncated.blockers ?? 0);
}
