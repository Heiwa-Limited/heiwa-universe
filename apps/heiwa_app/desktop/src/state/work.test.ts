import { describe, expect, it } from "vitest";
import { createWorkState } from "./work";
import {
  catalogEvidence,
  describeRun,
  runSummaryText,
  runsFrom,
  statusLabel,
  summarizeRuns,
  workRecord,
  type WorkCatalog,
  type WorkSurfaceView,
} from "./work-model";

type Deferred = { path: string; resolve: (value: unknown) => void; reject: (error: unknown) => void };

/** A runtime whose replies the test releases in any order. */
function controllableRuntime() {
  const pending: Deferred[] = [];
  const get = (path: string) => new Promise<unknown>((resolve, reject) => {
    pending.push({ path, resolve, reject });
  });
  const take = (fragment: string): Deferred => {
    const index = pending.findIndex((request) => request.path.includes(fragment));
    if (index < 0) throw new Error(`no pending request for ${fragment}; pending: ${pending.map((r) => r.path).join(", ")}`);
    return pending.splice(index, 1)[0];
  };
  return { get, take, pending };
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

function catalogPayload(rows: Array<{ id: string; intent: string; status?: string; updated?: string; revision?: number }>, extra: Record<string, number> = {}) {
  return {
    ok: true,
    data: {
      work: rows.map((row) => ({
        work_id: row.id,
        intent: row.intent,
        status: row.status ?? "active",
        revision: row.revision ?? 1,
        primary_thread_id: `thread-${row.id}`,
        related_thread_ids: [],
        created_at: "2026-09-14T00:00:00+00:00",
        updated_at: row.updated ?? "2026-09-14T00:00:00+00:00",
      })),
      total: extra.total ?? rows.length,
      truncated: extra.truncated ?? 0,
      skipped_events: extra.skipped_events ?? 0,
    },
  };
}

type SurfaceOptions = {
  status?: string;
  workRevision?: number;
  epoch?: string;
  revision?: number;
  cursor?: string | null;
  runs?: Record<string, Record<string, unknown>>;
  truncated?: Record<string, number>;
  intent?: string;
  identityOverride?: Partial<Record<"home" | "work" | "agent", Record<string, unknown>>>;
  omit?: "home" | "work" | "agent";
};

function surfacesPayload(workId: string, options: SurfaceOptions = {}) {
  const identity = {
    work_id: workId,
    work_revision: options.workRevision ?? 2,
    projection_epoch: options.epoch ?? "epoch-a",
    projection_revision: options.revision ?? 7,
    operator_cursor: options.cursor === undefined ? "cursor-7" : options.cursor,
  };
  const collections = {
    work: { [workId]: { work_id: workId, intent: options.intent ?? `Objective for ${workId}`, status: options.status ?? "active", primary_thread_id: `thread-${workId}`, related_thread_ids: [], updated_at: "2026-09-14T00:00:00+00:00" } },
    runs: options.runs ?? {},
    blockers: {},
  };
  const view = (surface: "home" | "work" | "agent") => ({
    surface,
    identity: { ...identity, ...(options.identityOverride?.[surface] ?? {}) },
    collections,
    truncated_collections: options.truncated ?? {},
  });
  const names = (["home", "work", "agent"] as const).filter((name) => name !== options.omit);
  return { ok: true, data: { surfaces: names.map(view) } };
}

const run = (runId: string, fields: Record<string, unknown>) => ({
  run_id: runId,
  worker_id: "worker-1",
  work_id: "work-a",
  thread_id: "thread-work-a",
  worker_state: "exited",
  provider: "local",
  started_at: "2026-09-14T00:00:00+00:00",
  ended_at: null,
  exit_code: null,
  failure_code: null,
  pid: null,
  supervision: null,
  pane_state: null,
  ...fields,
});

describe("Work catalog", () => {
  it("distinguishes an empty installation from a failed load and keeps a known catalog through a failed refresh", async () => {
    const runtime = controllableRuntime();
    const state = createWorkState({ get: runtime.get });

    const firstFailure = state.loadCatalog();
    runtime.take("/operator/work").reject({ kind: "Offline", detail: "runtime request failed" });
    await firstFailure;
    expect(state.catalog().status).toBe("error");
    expect(state.catalog().catalog).toBeUndefined();
    expect(state.catalog().error?.kind).toBe("offline");

    const empty = state.loadCatalog();
    runtime.take("/operator/work").resolve(catalogPayload([]));
    await empty;
    expect(state.catalog()).toMatchObject({ status: "ready", stale: false });
    expect(state.catalog().error).toBeUndefined();
    expect(state.catalog().catalog?.rows).toEqual([]);

    const loaded = state.loadCatalog();
    runtime.take("/operator/work").resolve(catalogPayload([{ id: "work-a", intent: "Ship it" }], { total: 101, truncated: 100, skipped_events: 2 }));
    await loaded;
    expect(state.catalog().catalog).toMatchObject({ total: 101, truncated: 100, skippedEvents: 2, unreadableRows: 0 });

    const failedRefresh = state.loadCatalog();
    runtime.take("/operator/work").reject({ kind: "Http", detail: { status: 503, body: "{\"ok\":false}" } });
    await failedRefresh;
    expect(state.catalog()).toMatchObject({ status: "ready", stale: true });
    expect(state.catalog().error?.kind).toBe("unavailable");
    expect(state.catalog().catalog?.rows.map((row) => row.workId)).toEqual(["work-a"]);
  });

  it("counts rows it cannot read instead of inventing or silently dropping them", async () => {
    const runtime = controllableRuntime();
    const state = createWorkState({ get: runtime.get });
    const load = state.loadCatalog();
    const payload = catalogPayload([{ id: "work-a", intent: "Readable" }]);
    (payload.data.work as unknown[]).push({ intent: "no id" });
    runtime.take("/operator/work").resolve(payload);
    await load;
    expect(state.catalog().catalog?.rows).toHaveLength(1);
    expect(state.catalog().catalog?.unreadableRows).toBe(1);
  });
});

describe("Work detail", () => {
  it("never lets a late reply for Work A replace a later selection of Work B", async () => {
    const runtime = controllableRuntime();
    const state = createWorkState({ get: runtime.get });

    const selectA = state.select("work-a");
    const selectB = state.select("work-b");
    runtime.take("/work/work-b/").resolve(surfacesPayload("work-b"));
    await selectB;
    runtime.take("/work/work-a/").resolve(surfacesPayload("work-a"));
    await selectA;

    expect(state.selectedId()).toBe("work-b");
    expect(state.detail().workId).toBe("work-b");
    expect(state.detail().surfaces?.identity.workId).toBe("work-b");
  });

  it("never lets an older refresh of the same Work replace a newer accepted snapshot", async () => {
    const runtime = controllableRuntime();
    const state = createWorkState({ get: runtime.get });
    const initial = state.select("work-a");
    runtime.take("/work/work-a/").resolve(surfacesPayload("work-a", { revision: 1 }));
    await initial;

    const older = state.refresh();
    const newer = state.refresh();
    runtime.pending[1].resolve(surfacesPayload("work-a", { revision: 9, cursor: "cursor-9" }));
    runtime.pending.splice(1, 1);
    await newer;
    runtime.take("/work/work-a/").resolve(surfacesPayload("work-a", { revision: 3, cursor: "cursor-3" }));
    await older;

    expect(state.detail().surfaces?.identity).toMatchObject({ projectionRevision: 9, operatorCursor: "cursor-9" });
  });

  it("replaces the whole snapshot when the runtime restarts into a new epoch, even at a lower revision", async () => {
    const runtime = controllableRuntime();
    const state = createWorkState({ get: runtime.get });
    const initial = state.select("work-a");
    runtime.take("/work/work-a/").resolve(surfacesPayload("work-a", { epoch: "epoch-a", revision: 40, intent: "Before restart" }));
    await initial;

    const restarted = state.refresh();
    runtime.take("/work/work-a/").resolve(surfacesPayload("work-a", { epoch: "epoch-b", revision: 3, intent: "After restart" }));
    await restarted;

    const surfaces = state.detail().surfaces!;
    expect(surfaces.identity).toMatchObject({ projectionEpoch: "epoch-b", projectionRevision: 3 });
    expect(workRecord(surfaces).intent).toBe("After restart");
    expect(surfaces.agent.identity.projectionEpoch).toBe("epoch-b");
  });

  it("refuses surfaces that disagree, name another Work, or are incomplete, and keeps the last good snapshot marked stale", async () => {
    const runtime = controllableRuntime();
    const state = createWorkState({ get: runtime.get });
    const initial = state.select("work-a");
    runtime.take("/work/work-a/").resolve(surfacesPayload("work-a", { revision: 5 }));
    await initial;

    for (const bad of [
      surfacesPayload("work-a", { identityOverride: { agent: { projection_epoch: "epoch-other" } } }),
      surfacesPayload("work-b"),
      surfacesPayload("work-a", { omit: "agent" }),
      { ok: true, data: { surfaces: "not a list" } },
    ]) {
      const attempt = state.refresh();
      runtime.take("/work/work-a/").resolve(bad);
      await attempt;
      expect(state.detail().error?.kind).toBe("incompatible");
      expect(state.detail().stale).toBe(true);
      expect(state.detail().surfaces?.identity.projectionRevision).toBe(5);
    }
  });

  it("shows retained data as stale after a transient failure and clears the error only when a valid reply arrives", async () => {
    const runtime = controllableRuntime();
    const state = createWorkState({ get: runtime.get });
    const initial = state.select("work-a");
    runtime.take("/work/work-a/").resolve(surfacesPayload("work-a", { revision: 2 }));
    await initial;

    const failed = state.refresh();
    runtime.take("/work/work-a/").reject({ kind: "Offline", detail: "runtime request failed" });
    await failed;
    expect(state.detail()).toMatchObject({ stale: true, loading: false });
    expect(state.detail().error?.kind).toBe("offline");

    const retry = state.refresh();
    await settle();
    expect(state.detail().loading).toBe(true);
    expect(state.detail().error?.kind).toBe("offline");
    runtime.take("/work/work-a/").resolve(surfacesPayload("work-a", { revision: 3 }));
    await retry;
    expect(state.detail()).toMatchObject({ stale: false, loading: false });
    expect(state.detail().error).toBeUndefined();
    expect(state.detail().surfaces?.identity.projectionRevision).toBe(3);
  });

  it("treats a failure with no prior snapshot as an error, not as an empty Work", async () => {
    const runtime = controllableRuntime();
    const state = createWorkState({ get: runtime.get });
    const attempt = state.select("work-a");
    runtime.take("/work/work-a/").reject({ kind: "Offline", detail: "down" });
    await attempt;
    expect(state.detail()).toMatchObject({ workId: "work-a", stale: false });
    expect(state.detail().surfaces).toBeUndefined();
    expect(state.detail().error?.kind).toBe("offline");
  });

  it("drops a Work the runtime no longer knows instead of showing it as stale", async () => {
    const runtime = controllableRuntime();
    const state = createWorkState({ get: runtime.get });
    const initial = state.select("work-a");
    runtime.take("/work/work-a/").resolve(surfacesPayload("work-a"));
    await initial;

    const gone = state.refresh();
    runtime.take("/work/work-a/").reject({ kind: "Http", detail: { status: 404, body: "{\"ok\":false,\"error\":{\"code\":\"unknown_work\"}}" } });
    await gone;
    expect(state.detail().surfaces).toBeUndefined();
    expect(state.detail().stale).toBe(false);
    expect(state.detail().error?.kind).toBe("not_found");
  });

  it("accepts no replies after disposal", async () => {
    const runtime = controllableRuntime();
    const state = createWorkState({ get: runtime.get });
    const pending = state.select("work-a");
    const catalog = state.loadCatalog();
    state.dispose();
    runtime.take("/work/work-a/").resolve(surfacesPayload("work-a"));
    runtime.take("/operator/work").resolve(catalogPayload([{ id: "work-a", intent: "late" }]));
    await Promise.all([pending, catalog]);
    expect(state.detail().surfaces).toBeUndefined();
    expect(state.catalog().catalog).toBeUndefined();
  });

  it("labels bounded collections from the snapshot", async () => {
    const runtime = controllableRuntime();
    const state = createWorkState({ get: runtime.get });
    const attempt = state.select("work-a");
    runtime.take("/work/work-a/").resolve(surfacesPayload("work-a", { truncated: { runs: 12, artifacts: 3 } }));
    await attempt;
    expect(state.detail().surfaces?.agent.truncated).toEqual({ runs: 12, artifacts: 3 });
  });
});

describe("Work runs", () => {
  it("keeps repeated runs of one worker separate and newest first", () => {
    const payload = surfacesPayload("work-a", { runs: {
      "run-1": run("run-1", { worker_state: "exited", exit_code: 0, started_at: "2026-09-14T01:00:00+00:00" }),
      "run-2": run("run-2", { worker_state: "live", pid: 44, started_at: "2026-09-14T02:00:00+00:00" }),
    } });
    const agent = payload.data.surfaces[2];
    const { runs, unreadable } = runsFrom({
      surface: "agent",
      identity: { workId: "work-a", workRevision: 2, projectionEpoch: "epoch-a", projectionRevision: 7, operatorCursor: "cursor-7" },
      collections: agent.collections as never,
      truncated: {},
    });
    expect(unreadable).toBe(0);
    expect(runs.map((item) => item.runId)).toEqual(["run-2", "run-1"]);
    expect(new Set(runs.map((item) => item.key)).size).toBe(2);
    expect(runs.every((item) => item.workerId === "worker-1")).toBe(true);
    expect(describeRun(runs[0]).label).toBe("Running");
    expect(describeRun(runs[1]).label).toBe("Finished");
  });

  it("describes lost supervision as a dated observation, never as stopped or running", () => {
    const at = "2026-09-14T03:04:00+00:00";
    const describe_ = (process: string) => describeRun(runsFrom({
      surface: "agent",
      identity: { workId: "work-a", workRevision: 1, projectionEpoch: "e", projectionRevision: 1, operatorCursor: null },
      collections: { runs: { r: run("r", { worker_state: "stale", pid: 9, supervision: { reason: "owner_lost", process, pid: 9, recorded_at: at } }) } },
      truncated: {},
    }).runs[0]);

    const alive = describe_("alive");
    expect(alive.label).toBe("Supervision lost");
    expect(alive.tone).toBe("attention");
    expect(alive.supervision?.observation).toMatch(/still running when recovery checked at/);
    expect(alive.supervision?.observation).toMatch(/did not stop it/);
    expect(`${alive.label} ${alive.supervision?.observation}`).not.toMatch(/\b(stopped|recovered|contained|is running|currently running)\b/i);

    expect(describe_("gone").supervision?.observation).toMatch(/no longer running when recovery checked at .*No exit was recorded/);
    expect(describe_("unknown").supervision?.observation).toMatch(/could not tell whether the process was running/);
    expect(describe_("reattached").supervision?.observation).toMatch(/does not recognize/);
  });

  it("keeps a supervision loss as history when a later exit ends the run", () => {
    const { runs } = runsFrom({
      surface: "agent",
      identity: { workId: "work-a", workRevision: 1, projectionEpoch: "e", projectionRevision: 1, operatorCursor: null },
      collections: { runs: { r: run("r", { worker_state: "exited", exit_code: 0, supervision: { reason: "owner_lost", process: "alive", pid: 9, recorded_at: "2026-09-14T03:04:00+00:00" } }) } },
      truncated: {},
    });
    const description = describeRun(runs[0]);
    expect(description.label).toBe("Finished");
    expect(description.supervision?.headline).toBe("Supervision lost");
  });

  it("never presents an unrecognized run or Work state as finished", () => {
    const { runs, unreadable } = runsFrom({
      surface: "agent",
      identity: { workId: "work-a", workRevision: 1, projectionEpoch: "e", projectionRevision: 1, operatorCursor: null },
      collections: { runs: { r: run("r", { worker_state: "teleported" }), broken: { worker_state: "exited" } } },
      truncated: {},
    });
    expect(unreadable).toBe(1);
    expect(describeRun(runs[0])).toMatchObject({ tone: "unknown" });
    expect(describeRun(runs[0]).label).toMatch(/Unrecognized state/);
    expect(statusLabel("unrecognized", "archived")).toMatch(/Unrecognized status/);
  });
});

describe("discovery and detail agreement", () => {
  it("shows a snapshot's status over an older catalog row and never lets an older catalog downgrade it", async () => {
    const runtime = controllableRuntime();
    const state = createWorkState({ get: runtime.get });
    const firstCatalog = state.loadCatalog();
    runtime.take("/operator/work").resolve(catalogPayload([{ id: "work-a", intent: "Ship", revision: 2 }]));
    await firstCatalog;

    const selected = state.select("work-a");
    runtime.take("/work/work-a/").resolve(surfacesPayload("work-a", { status: "complete", workRevision: 3 }));
    await selected;
    expect(state.rows()[0]).toMatchObject({ status: "complete", revision: 3 });

    const olderCatalog = state.loadCatalog();
    runtime.take("/operator/work").resolve(catalogPayload([{ id: "work-a", intent: "Ship", revision: 2 }]));
    await olderCatalog;
    expect(state.rows()[0].status).toBe("complete");
    expect(runtime.pending).toHaveLength(0);
  });

  it("reloads the selected Work when the catalog proves it changed since its snapshot", async () => {
    const runtime = controllableRuntime();
    const state = createWorkState({ get: runtime.get });
    const selected = state.select("work-a");
    runtime.take("/work/work-a/").resolve(surfacesPayload("work-a", { workRevision: 2 }));
    await selected;

    const newer = state.loadCatalog();
    runtime.take("/operator/work").resolve(catalogPayload([{ id: "work-a", intent: "Ship", status: "blocked", revision: 5 }]));
    await settle();
    // Until the new snapshot arrives, the newer catalog row is what is shown.
    expect(state.rows()[0]).toMatchObject({ status: "blocked", revision: 5 });
    runtime.take("/work/work-a/").resolve(surfacesPayload("work-a", { status: "blocked", workRevision: 5 }));
    await newer;
    expect(state.detail().surfaces?.identity.workRevision).toBe(5);
  });

  it("files a late reply under its own Work without touching the selected one", async () => {
    const runtime = controllableRuntime();
    const state = createWorkState({ get: runtime.get });
    const catalog = state.loadCatalog();
    runtime.take("/operator/work").resolve(catalogPayload([{ id: "work-a", intent: "A" }, { id: "work-b", intent: "B" }]));
    await catalog;
    const selectA = state.select("work-a");
    const selectB = state.select("work-b");
    runtime.take("/work/work-b/").resolve(surfacesPayload("work-b", { intent: "B detail" }));
    await selectB;
    runtime.take("/work/work-a/").resolve(surfacesPayload("work-a", { status: "complete", workRevision: 9 }));
    await selectA;
    expect(state.detail().workId).toBe("work-b");
    expect(workRecord(state.detail().surfaces!).intent).toBe("B detail");
    expect(state.rows().find((row) => row.workId === "work-a")?.status).toBe("complete");
  });
});

describe("summaries that never overclaim", () => {
  const view = (runs: Record<string, Record<string, unknown>>, truncated: Record<string, number> = {}): WorkSurfaceView => ({
    surface: "agent",
    identity: { workId: "work-a", workRevision: 1, projectionEpoch: "e", projectionRevision: 1, operatorCursor: null },
    collections: { runs },
    truncated,
  });

  it("requires positive evidence before calling a catalog empty", () => {
    const base: WorkCatalog = { rows: [], total: 0, truncated: 0, skippedEvents: 0, unreadableRows: 0 };
    expect(catalogEvidence(undefined)).toBe("unknown");
    expect(catalogEvidence(base)).toBe("empty");
    expect(catalogEvidence({ ...base, unreadableRows: 1, total: 1 })).toBe("uncertain");
    expect(catalogEvidence({ ...base, skippedEvents: 1 })).toBe("uncertain");
    expect(catalogEvidence({ ...base, total: 2 })).toBe("uncertain");
    expect(catalogEvidence({ ...base, truncated: 4, total: 4 })).toBe("uncertain");
  });

  it("counts failed, lost, and unrecognized runs, and admits what it cannot see", () => {
    const failed = summarizeRuns(view({ f: run("f", { worker_state: "failed", failure_code: "build_failed", exit_code: 1 }) }));
    expect(runSummaryText(failed)).toBe("1 failed.");
    const nonzero = summarizeRuns(view({ n: run("n", { worker_state: "exited", exit_code: 2 }) }));
    expect(runSummaryText(nonzero)).toBe("1 failed.");
    const fine = summarizeRuns(view({ ok: run("ok", { exit_code: 0 }), live: run("live", { worker_state: "live" }) }));
    expect(runSummaryText(fine)).toBe("None need attention.");
    expect(runSummaryText(summarizeRuns(view({ ok: run("ok", { exit_code: 0 }), broken: { worker_state: "exited" } })))).toMatch(/some run records are not shown/);
    expect(runSummaryText(summarizeRuns(view({ ok: run("ok", { exit_code: 0 }) }, { runs: 5 })))).toMatch(/some run records are not shown/);
    expect(runSummaryText(summarizeRuns(view({})))).toBe("No runs recorded.");
    expect(runSummaryText(summarizeRuns(view({}, { runs: 3 })))).not.toBe("No runs recorded.");
  });
});

describe("per-Work detail state on shared rows", () => {
  it("carries a Work's refresh failure on its row until that Work's retry succeeds", async () => {
    const runtime = controllableRuntime();
    const state = createWorkState({ get: runtime.get });
    const catalog = state.loadCatalog({ prefetch: 3 });
    runtime.take("/operator/work").resolve(catalogPayload([{ id: "work-a", intent: "A" }, { id: "work-b", intent: "B" }]));
    await settle();
    runtime.take("/work/work-a/").resolve(surfacesPayload("work-a"));
    runtime.take("/work/work-b/").resolve(surfacesPayload("work-b"));
    await catalog;

    const again = state.loadCatalog({ prefetch: 3 });
    runtime.take("/operator/work").resolve(catalogPayload([{ id: "work-a", intent: "A" }, { id: "work-b", intent: "B" }]));
    await settle();
    runtime.take("/work/work-a/").reject({ kind: "Offline", detail: "down" });
    runtime.take("/work/work-b/").resolve(surfacesPayload("work-b"));
    await again;

    const rowA = () => state.rows().find((row) => row.workId === "work-a")!;
    const rowB = () => state.rows().find((row) => row.workId === "work-b")!;
    expect(state.catalog().error).toBeUndefined();
    expect(rowA().detail).toMatchObject({ stale: true, loading: false });
    expect(rowA().detail?.error?.kind).toBe("offline");
    expect(rowA().snapshot).toBeDefined();
    expect(rowB().detail?.error).toBeUndefined();
    expect(state.selectedId()).toBeUndefined();

    const retry = state.retry("work-a");
    await settle();
    expect(rowA().detail).toMatchObject({ loading: true, stale: true });
    expect(rowA().detail?.error?.kind).toBe("offline");
    runtime.take("/work/work-a/").resolve(surfacesPayload("work-a", { revision: 8 }));
    await retry;
    expect(rowA().detail).toMatchObject({ loading: false, stale: false });
    expect(rowA().detail?.error).toBeUndefined();
    expect(state.selectedId()).toBeUndefined();
  });
});
