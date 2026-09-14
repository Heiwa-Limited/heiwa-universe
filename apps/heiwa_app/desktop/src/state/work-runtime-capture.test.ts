import { describe, expect, it } from "vitest";
import capture from "./fixtures/work-runtime-capture.json";
import { classifyWorkError } from "./work";
import { describeRun, parseWorkCatalog, parseWorkSurfaces, runsFrom, workRecord } from "./work-model";

/**
 * The desktop's validation against what the runtime actually sends. The
 * fixture was produced by a real `heiwa app start` over disposable Work, so a
 * change on either side of the route that breaks the contract fails here.
 */
describe("runtime-captured Work payloads", () => {
  it("reads the real catalog", () => {
    const catalog = parseWorkCatalog(capture.catalog);
    expect(catalog).toMatchObject({ total: 3, truncated: 0, skippedEvents: 0, unreadableRows: 0 });
    expect(catalog.rows.map((row) => row.intent)).toEqual([
      "Draft release notes",
      "Summarize the calendar plan",
      "Repair the recovery report",
    ]);
    expect(catalog.rows.every((row) => row.status === "active" && row.primaryThreadId.startsWith("thread-"))).toBe(true);
  });

  it("accepts each real surfaces response for its own Work only", () => {
    const { ids, surfaces } = capture;
    const alive = parseWorkSurfaces(surfaces["stale-alive-and-finished"], ids.stale_alive_and_finished);
    expect(workRecord(alive).intent).toBe("Repair the recovery report");
    expect(() => parseWorkSurfaces(surfaces["stale-alive-and-finished"], ids.stale_gone)).toThrow(/another Work/);

    const runs = runsFrom(alive.agent);
    expect(runs.unreadable).toBe(0);
    expect(runs.runs).toHaveLength(2);
    expect(new Set(runs.runs.map((run) => run.workerId)).size).toBe(1);
    const described = runs.runs.map(describeRun);
    expect(described.map((item) => item.label).sort()).toEqual(["Finished", "Supervision lost"]);
    expect(described.find((item) => item.label === "Supervision lost")?.supervision?.observation).toMatch(/still running when recovery checked at/);

    const gone = runsFrom(parseWorkSurfaces(surfaces["stale-gone"], ids.stale_gone).agent);
    expect(describeRun(gone.runs[0]).supervision?.observation).toMatch(/no longer running when recovery checked at/);

    const empty = parseWorkSurfaces(surfaces["no-runs"], ids.no_runs);
    expect(runsFrom(empty.agent).runs).toEqual([]);
  });

  it("recognizes the runtime's real unknown-Work reply as authoritative", () => {
    expect(classifyWorkError({ kind: "Http", detail: { status: 404, body: capture.unknown_work_http_body } }).kind).toBe("not_found");
  });
});
