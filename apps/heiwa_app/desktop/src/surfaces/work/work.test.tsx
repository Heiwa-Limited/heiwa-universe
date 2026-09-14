// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { App } from "../../app";
import { createAppState } from "../../state/app";

afterEach(cleanup);

type Reply = { resolve: (value: unknown) => void; reject: (error: unknown) => void };

/**
 * App state with every runtime seam faked. Work requests wait in `pending`
 * until the test answers them, so reply order is the test's choice.
 */
function workApp(options: { catalog?: unknown; catalogError?: unknown } = {}) {
  const pending = new Map<string, Reply[]>();
  const workGet = (path: string) => {
    if (path === "/api/v1/operator/work") {
      return options.catalogError ? Promise.reject(options.catalogError) : Promise.resolve(options.catalog ?? catalog([]));
    }
    return new Promise<unknown>((resolve, reject) => {
      pending.set(path, [...(pending.get(path) ?? []), { resolve, reject }]);
    });
  };
  /** Answer every request waiting for this Work; superseded ones are ignored by the service. */
  const answer = async (workId: string, reply: unknown, fail = false) => {
    const path = `/api/v1/operator/work/${workId}/surfaces`;
    await waitFor(() => expect(pending.get(path)?.length ?? 0).toBeGreaterThan(0));
    for (const next of pending.get(path)!.splice(0)) {
      if (fail) next.reject(reply); else next.resolve(reply);
    }
  };
  const waiting = (workId: string) => pending.get(`/api/v1/operator/work/${workId}/surfaces`)?.length ?? 0;
  const state = createAppState({
    work: { get: workGet },
    sessions: { get: vi.fn().mockResolvedValue({ ok: true, data: { threads: [], projects: [], truncated: false } }) as never },
    operator: {
      get: vi.fn().mockResolvedValue({ ok: true, data: { events: [], next_cursor: null, skipped_lines: 0 } }),
      post: vi.fn().mockResolvedValue({ ok: true, data: {} }),
      subscribe: () => new Promise<void>(() => {}),
      randomUUID: () => "req-1",
      schedule: (task) => task(),
    },
    runtime: {
      get: vi.fn().mockResolvedValue({ data: { items: [], holds: [], events: [] } }) as never,
      health: vi.fn().mockResolvedValue({ reachable: true, error: null, snapshot: { ok: true, data: {} } }),
      post: vi.fn().mockResolvedValue({ ok: true, data: {} }),
      readAppleMail: vi.fn().mockRejectedValue(new Error("not in this test")),
    },
    herd: {
      snapshot: vi.fn().mockResolvedValue({ status: "online", source: "test", panes: [], error: null }),
      catalog: vi.fn().mockResolvedValue([]),
      read: vi.fn().mockResolvedValue({ ok: true, pane: "", text: "", source: "test", error: null }),
    },
  });
  return { state, answer, waiting };
}

function catalog(rows: Array<{ id: string; intent: string; status?: string; updated: string }>, bounds: { total?: number; truncated?: number; skipped?: number } = {}) {
  return {
    ok: true,
    data: {
      work: rows.map((row) => ({
        work_id: row.id, intent: row.intent, status: row.status ?? "active", revision: 1,
        primary_thread_id: `thread-${row.id}`, related_thread_ids: [], updated_at: row.updated,
      })),
      total: bounds.total ?? rows.length,
      truncated: bounds.truncated ?? 0,
      skipped_events: bounds.skipped ?? 0,
    },
  };
}

function surfaces(workId: string, intent: string, extra: { runs?: Record<string, unknown>; bounds?: Record<string, number>; blockers?: Record<string, unknown> } = {}) {
  const identity = { work_id: workId, work_revision: 3, projection_epoch: "epoch-1", projection_revision: 9, operator_cursor: "cursor-9" };
  const collections = {
    work: { [workId]: { work_id: workId, intent, status: "active", primary_thread_id: `thread-${workId}`, related_thread_ids: [], updated_at: "2026-09-14T02:00:00+00:00" } },
    threads: { [`thread-${workId}`]: { thread_id: `thread-${workId}`, status: "completed", updated_at: "2026-09-14T01:30:00+00:00" } },
    runs: extra.runs ?? {},
    blockers: extra.blockers ?? {},
  };
  return {
    ok: true,
    data: {
      surfaces: (["home", "work", "agent"] as const).map((surface) => ({
        surface, identity, collections, truncated_collections: extra.bounds ?? {},
      })),
    },
  };
}

const runRow = (runId: string, fields: Record<string, unknown>) => ({
  run_id: runId, worker_id: "worker-7", work_id: "work-a", thread_id: "thread-work-a",
  worker_state: "exited", provider: "codex", started_at: "2026-09-14T01:00:00+00:00",
  ended_at: null, exit_code: null, failure_code: null, pid: null, supervision: null, pane_state: null,
  ...fields,
});

describe("desktop Work", () => {
  it("navigates from Home to a Work's detail and on to its runs, describing lost supervision truthfully", async () => {
    const { state, answer, waiting } = workApp({
      catalog: catalog([
        { id: "work-done", intent: "Archive old notes", status: "complete", updated: "2026-09-14T03:00:00+00:00" },
        { id: "work-a", intent: "Repair recovery report", updated: "2026-09-14T02:00:00+00:00" },
      ]),
    });
    render(() => <App state={state} />);

    const snapshot = surfaces("work-a", "Repair recovery report", {
      runs: {
        "run-1": runRow("run-1", { exit_code: 0, ended_at: "2026-09-14T01:05:00+00:00" }),
        "run-2": runRow("run-2", {
          worker_state: "stale", pid: 4242, started_at: "2026-09-14T01:10:00+00:00",
          supervision: { reason: "owner_lost", process: "alive", pid: 4242, recorded_at: "2026-09-14T01:20:00+00:00" },
        }),
      },
      blockers: { b1: { event_id: "b1", code: "approval_required", reason: "Needs a decision on the push", occurred_at: "2026-09-14T01:30:00+00:00" } },
    });
    // Home reads the unfinished Work's Home projection, not just its catalog row.
    await answer("work-a", snapshot);
    const homeWork = await screen.findByRole("heading", { name: "Work" });
    const section = homeWork.closest("section")!;
    const rows = () => within(section).getAllByRole("button").filter((button) => button.classList.contains("home-session-row"));
    await waitFor(() => expect(rows()[0].textContent).toMatch(/1 lost supervision · 1 blocker/));
    expect(rows().map((row) => row.textContent)).toEqual([
      expect.stringContaining("Repair recovery report"),
      expect.stringContaining("Archive old notes"),
    ]);

    fireEvent.click(rows()[0]);
    await answer("work-a", snapshot);

    expect(await screen.findByRole("heading", { name: "Repair recovery report", level: 2 })).toBeTruthy();
    expect(screen.getByText("1 lost supervision.")).toBeTruthy();
    expect(screen.getByText("Needs a decision on the push")).toBeTruthy();
    expect(screen.getByText(/Last recorded turn: completed/)).toBeTruthy();
    expect(screen.getByRole("button", { name: /Repair recovery report/ }).getAttribute("aria-current")).toBe("true");

    fireEvent.click(screen.getByRole("button", { name: /View runs in Workers/ }));
    const runs = await screen.findByRole("region", { name: "Work runs" });
    expect(within(runs).getByText("Runs for “Repair recovery report”")).toBeTruthy();
    const items = within(runs).getAllByRole("listitem");
    expect(items).toHaveLength(2);
    expect(items[0].textContent).toMatch(/Supervision lost/);
    expect(within(items[0]).getByRole("note").textContent).toMatch(/still running when recovery checked at .*did not stop it/);
    expect(items[1].textContent).toMatch(/Finished/);
    expect(runs.textContent).not.toMatch(/\b(stopped|recovered|contained|currently running)\b/i);
    expect(within(runs).queryByRole("button", { name: /kill|stop|relaunch|reattach/i })).toBeNull();
  });

  it("shows the Work selected last even when an earlier selection answers late", async () => {
    const { state, answer } = workApp({
      catalog: catalog([
        { id: "work-a", intent: "First goal", updated: "2026-09-14T02:00:00+00:00" },
        { id: "work-b", intent: "Second goal", updated: "2026-09-14T01:00:00+00:00" },
      ]),
    });
    state.navigate("work");
    render(() => <App state={state} />);

    fireEvent.click(await screen.findByRole("button", { name: /First goal/ }));
    fireEvent.click(screen.getByRole("button", { name: /Second goal/ }));
    await answer("work-b", surfaces("work-b", "Second goal"));
    await answer("work-a", surfaces("work-a", "First goal"));

    expect(await screen.findByRole("heading", { name: "Second goal", level: 2 })).toBeTruthy();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(screen.queryByRole("heading", { name: "First goal", level: 2 })).toBeNull();
  });

  it("keeps a Work visibly stale when a refresh fails and clears the warning only after a valid retry", async () => {
    const { state, answer } = workApp({ catalog: catalog([{ id: "work-a", intent: "Keep going", updated: "2026-09-14T02:00:00+00:00" }]) });
    state.navigate("work");
    render(() => <App state={state} />);

    fireEvent.click(await screen.findByRole("button", { name: /Keep going/ }));
    await answer("work-a", surfaces("work-a", "Keep going"));
    expect(await screen.findByRole("heading", { name: "Keep going", level: 2 })).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
    await answer("work-a", { kind: "Offline", detail: "runtime request failed" }, true);
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toMatch(/Showing an earlier snapshot\. The Heiwa runtime is not reachable\./);
    expect(screen.getByRole("heading", { name: "Keep going", level: 2 })).toBeTruthy();

    fireEvent.click(within(alert).getByRole("button", { name: "Retry" }));
    await answer("work-a", surfaces("work-a", "Keep going, refreshed"));
    expect(await screen.findByRole("heading", { name: "Keep going, refreshed", level: 2 })).toBeTruthy();
    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  });

  it("never renders an incompatible snapshot and never reports a failed catalog as no Work", async () => {
    const offline = workApp({ catalogError: { kind: "Offline", detail: "down" } });
    offline.state.navigate("work");
    render(() => <App state={offline.state} />);
    expect((await screen.findByRole("alert")).textContent).toMatch(/not reachable/);
    expect(screen.queryByText(/No Work has been created/)).toBeNull();
    cleanup();

    const { state, answer } = workApp({ catalog: catalog([{ id: "work-a", intent: "Guarded", updated: "2026-09-14T02:00:00+00:00" }]) });
    state.navigate("work");
    render(() => <App state={state} />);
    fireEvent.click(await screen.findByRole("button", { name: /Guarded/ }));
    const mismatched = surfaces("work-a", "Should not render");
    (mismatched.data.surfaces[2] as { identity: Record<string, unknown> }).identity = { ...mismatched.data.surfaces[2].identity, projection_epoch: "other" };
    await answer("work-a", mismatched);
    expect((await screen.findByRole("alert")).textContent).toMatch(/cannot read/);
    expect(screen.queryByRole("heading", { name: "Should not render" })).toBeNull();
  });

  it("labels bounded catalogs, skipped events, and omitted runs", async () => {
    const { state, answer } = workApp({
      catalog: catalog([{ id: "work-a", intent: "Big history", updated: "2026-09-14T02:00:00+00:00" }], { total: 140, truncated: 139, skipped: 2 }),
    });
    state.navigate("work");
    render(() => <App state={state} />);

    expect(await screen.findByText("Showing the 1 most recently updated of 140 Work.")).toBeTruthy();
    expect(screen.getByText(/2 Work event\(s\) could not be read/)).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: /Big history/ }));
    await answer("work-a", surfaces("work-a", "Big history", { runs: { r: runRow("r", { exit_code: 0 }) }, bounds: { runs: 12 } }));
    await screen.findByRole("heading", { name: "Big history", level: 2 });
    state.navigate("workers");
    expect(await screen.findByText("12 more runs not shown; the runtime bounds this list.")).toBeTruthy();
  });

  // Regressions from Astra's first desktop review (desktop-work-astra-review-1.md).
  it("does not declare an installation empty when its only Work row is unreadable", async () => {
    const unreadable = catalog([]);
    (unreadable.data.work as unknown[]).push({ work_id: "work-a" });
    unreadable.data.total = 1;
    const { state } = workApp({ catalog: unreadable });
    state.navigate("work");
    render(() => <App state={state} />);
    expect(await screen.findByText(/1 Work row\(s\) could not be read/)).toBeTruthy();
    expect(screen.queryByText("No Work has been created on this Mac yet.")).toBeNull();
    expect(screen.getByText(/runtime reports Work this app could not list/)).toBeTruthy();
  });

  it("keeps skipped-Work evidence on Home even when no Work row is readable", async () => {
    const { state } = workApp({ catalog: catalog([], { total: 0, skipped: 1 }) });
    render(() => <App state={state} />);
    expect(await screen.findByText(/1 Work event\(s\) could not be read/)).toBeTruthy();
  });

  it("does not say no runs need attention when a recorded run failed", async () => {
    const { state, answer } = workApp({ catalog: catalog([{ id: "work-a", intent: "Review failing build", updated: "2026-09-14T03:00:00+00:00" }]) });
    state.navigate("work");
    render(() => <App state={state} />);
    fireEvent.click(await screen.findByRole("button", { name: /Review failing build/ }));
    await answer("work-a", surfaces("work-a", "Review failing build", {
      runs: { "run-a": runRow("run-a", { worker_state: "failed", failure_code: "build_failed", exit_code: 1 }) },
    }));
    await screen.findByRole("heading", { name: "Review failing build", level: 2 });
    expect(screen.getByText("1 failed.")).toBeTruthy();
    expect(screen.queryByText("None need attention.")).toBeNull();
  });

  it("never shows a catalog status that contradicts a newer accepted snapshot, on the Work page or Home", async () => {
    const { state, answer } = workApp({ catalog: catalog([{ id: "work-a", intent: "Ship the fix", updated: "2026-09-14T03:00:00+00:00" }]) });
    state.navigate("work");
    render(() => <App state={state} />);
    fireEvent.click(await screen.findByRole("button", { name: /Ship the fix/ }));
    const complete = surfaces("work-a", "Ship the fix");
    const work = complete.data.surfaces[0].collections.work["work-a"] as Record<string, unknown>;
    work.status = "complete";
    for (const view of complete.data.surfaces) view.identity.work_revision = 4;
    await answer("work-a", complete);
    await screen.findByRole("heading", { name: "Ship the fix", level: 2 });
    expect(screen.getAllByText("Complete").length).toBeGreaterThan(0);
    expect(screen.queryByText("Active")).toBeNull();

    // An older catalog read (revision 1) must not downgrade what the newer snapshot showed.
    await state.work.loadCatalog();
    state.navigate("home");
    const home = (await screen.findByRole("heading", { name: "Work" })).closest("section")!;
    await waitFor(() => expect(within(home).getByText("Complete")).toBeTruthy());
    expect(screen.queryByText("Active")).toBeNull();
  });

  it("keeps keyboard focus on the Work row that was chosen while its snapshot arrives", async () => {
    const { state, answer } = workApp({ catalog: catalog([
      { id: "work-a", intent: "First goal", updated: "2026-09-14T02:00:00+00:00" },
      { id: "work-b", intent: "Second goal", updated: "2026-09-14T01:00:00+00:00" },
    ]) });
    state.navigate("work");
    render(() => <App state={state} />);
    const row = await screen.findByRole("button", { name: /Second goal/ });
    row.focus();
    fireEvent.click(row);
    const complete = surfaces("work-b", "Second goal");
    (complete.data.surfaces[0].collections.work["work-b"] as Record<string, unknown>).status = "complete";
    for (const view of complete.data.surfaces) view.identity.work_revision = 5;
    await answer("work-b", complete);
    await screen.findByRole("heading", { name: "Second goal", level: 2 });
    await waitFor(() => expect(within(row).getByText("Complete")).toBeTruthy());
    expect(document.activeElement).toBe(row);
  });

  it("reads at most three Work projections for Home", async () => {
    const rows = Array.from({ length: 6 }, (_, index) => ({
      id: `work-${index}`, intent: `Goal ${index}`, updated: `2026-09-14T0${9 - index}:00:00+00:00`,
    }));
    const { state, waiting } = workApp({ catalog: catalog(rows) });
    render(() => <App state={state} />);
    await screen.findByRole("heading", { name: "Work" });
    await waitFor(() => expect(rows.filter((row) => waiting(row.id) > 0)).toHaveLength(3));
    expect(rows.slice(3).every((row) => waiting(row.id) === 0)).toBe(true);
  });

  it("shows nothing about Work on Home for an installation that has none", async () => {
    const { state } = workApp();
    render(() => <App state={state} />);
    await screen.findByText("What’s on your mind?");
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(screen.queryByRole("heading", { name: "Work" })).toBeNull();
    expect(screen.queryByText(/All Work/)).toBeNull();
  });
});
