import { describe, expect, it, vi } from "vitest";
import type { AppleMailScanResult } from "../runtime";
import { createRuntimeState } from "./runtime";

describe("RuntimeState calendar", () => {
  const september = { from: "2026-08-30", to: "2026-10-10" };

  it("loads a local-day range and refreshes that same range later", async () => {
    const get = vi.fn().mockResolvedValue({
      data: { events: [{ id: "a", title: "A", date: "2026-09-14" }], sync: { status: "fresh", complete: true } },
    });
    const state = createRuntimeState({ get });

    await state.loadCalendar(september);
    await state.loadCalendar();

    expect(get).toHaveBeenNthCalledWith(1, "/api/v1/calendar/events?from=2026-08-30&to=2026-10-10");
    expect(get).toHaveBeenNthCalledWith(2, "/api/v1/calendar/events?from=2026-08-30&to=2026-10-10");
    expect(state.calendarEvents().map((event) => event.id)).toEqual(["a"]);
    expect(state.calendarSync()?.status).toBe("fresh");
    expect(state.calendarError()).toBeUndefined();
  });

  it("keeps the calendar the user is reading when a refresh fails", async () => {
    const get = vi.fn()
      .mockResolvedValueOnce({ data: { events: [{ id: "a", title: "A", date: "2026-09-14" }] } })
      .mockRejectedValueOnce(new Error("private runtime detail"));
    const state = createRuntimeState({ get });

    await state.loadCalendar(september);
    await state.loadCalendar();

    expect(state.calendarEvents().map((event) => event.id)).toEqual(["a"]);
    expect(state.calendarError()).toBe("The calendar could not be refreshed.");
  });

  it("ignores a slower answer for a month the user already left", async () => {
    let answerSeptember: (value: unknown) => void = () => {};
    const get = vi.fn((path: string) => path.includes("from=2026-08-30")
      ? new Promise((resolve) => { answerSeptember = resolve; })
      : Promise.resolve({ data: { events: [{ id: "oct", title: "October", date: "2026-10-02" }] } }));
    const state = createRuntimeState({ get: get as never });

    const slow = state.loadCalendar(september);
    await state.loadCalendar({ from: "2026-09-27", to: "2026-11-07" });
    answerSeptember({ data: { events: [{ id: "sep", title: "September", date: "2026-09-14" }] } });
    await slow;

    expect(state.calendarEvents().map((event) => event.id)).toEqual(["oct"]);
  });

  it("reloads rows only when the runtime actually read the calendar", async () => {
    const get = vi.fn().mockResolvedValue({ data: { events: [] } });
    const post = vi.fn()
      .mockResolvedValueOnce({ data: { status: "fresh", complete: true } })
      .mockResolvedValueOnce({ data: { status: "synced", complete: true, fetched: 3 } });
    const state = createRuntimeState({ get, post });

    await state.syncCalendar();
    expect(post).toHaveBeenLastCalledWith("/api/v1/calendar/sync", { max_age_seconds: 120, force: false });
    expect(get).not.toHaveBeenCalled();

    await state.syncCalendar({ force: true });
    expect(post).toHaveBeenLastCalledWith("/api/v1/calendar/sync", { max_age_seconds: 120, force: true });
    expect(get).toHaveBeenCalledOnce();
    expect(state.calendarSync()).toMatchObject({ status: "synced", fetched: 3 });
  });

  it("does not let a slower rows response roll back a newer sync outcome", async () => {
    let answerRows: (value: unknown) => void = () => {};
    const get = vi.fn()
      .mockImplementationOnce(() => new Promise((resolve) => { answerRows = resolve; }))
      .mockResolvedValue({ data: { events: [], sync: { status: "fresh", last_read_at: "2026-09-14T20:00:00Z" } } });
    const post = vi.fn().mockResolvedValue({ data: { status: "synced", last_read_at: "2026-09-14T20:00:00Z" } });
    const state = createRuntimeState({ get, post });

    const rows = state.loadCalendar(september);
    await state.syncCalendar({ force: true });
    answerRows({ data: { events: [], sync: { status: "fresh", last_read_at: "2026-09-14T19:50:00Z" } } });
    await rows;

    expect(state.calendarSync()?.last_read_at).toBe("2026-09-14T20:00:00Z");

    // A disconnect carries no timestamp and still has to land.
    get.mockResolvedValueOnce({ data: { events: [], sync: { status: "not_connected" } } });
    await state.loadCalendar();
    expect(state.calendarSync()?.status).toBe("not_connected");
  });

  it("shares one sync between callers that ask at the same time", async () => {
    let answer: (value: unknown) => void = () => {};
    const post = vi.fn(() => new Promise((resolve) => { answer = resolve; }));
    const state = createRuntimeState({ get: vi.fn().mockResolvedValue({ data: { events: [] } }), post: post as never });

    const focus = state.syncCalendar({ maxAgeSeconds: 30 });
    const interval = state.syncCalendar();
    expect(state.calendarSyncing()).toBe(true);
    answer({ data: { status: "fresh" } });
    await Promise.all([focus, interval]);

    expect(post).toHaveBeenCalledOnce();
    expect(state.calendarSyncing()).toBe(false);
  });

  it("reports an unreachable runtime as a sync error without throwing", async () => {
    const state = createRuntimeState({
      get: vi.fn(),
      post: vi.fn().mockRejectedValue(new Error("private runtime detail")),
    });

    await expect(state.syncCalendar()).resolves.toMatchObject({ status: "error" });
    expect(state.calendarSync()?.error).toBe("Calendar sync could not reach the Heiwa runtime.");
  });

  it("keeps a known connection and pending approvals through a failed refresh", async () => {
    const get = vi.fn()
      .mockResolvedValueOnce({ data: { source: "apple_calendar", status: "ready", calendars: [] } })
      .mockResolvedValueOnce({ data: { pending_count: 1, pending: [{ id: "req", action: "a", target: "t", risk: "T2" }] } })
      .mockRejectedValue(new Error("private runtime detail"));
    const state = createRuntimeState({ get });

    await state.loadCalendarResources();
    await state.loadApprovals();
    await state.loadCalendarResources();
    await state.loadApprovals();

    expect(state.calendarResources()?.status).toBe("ready");
    expect(state.approvals()?.pending_count).toBe(1);
  });
});

describe("RuntimeState Apple Mail", () => {
  it("refreshes the local snapshot after an explicit successful read", async () => {
    const get = vi.fn().mockResolvedValue({
      data: { priority: [{ sender: "ada@example.com", subject: "Plan", unread: true }] },
    });
    const readAppleMail = vi.fn().mockResolvedValue({ fetched: 4, appended: 1, deduplicated: 3 });
    const state = createRuntimeState({ get, readAppleMail });

    await expect(state.readAppleMail()).resolves.toEqual({ fetched: 4, appended: 1, deduplicated: 3 });
    expect(readAppleMail).toHaveBeenCalledOnce();
    expect(get).toHaveBeenCalledWith("/api/v1/mail/summary");
    expect(state.mail()).toEqual([{ sender: "ada@example.com", subject: "Plan", unread: true }]);
    expect(state.mailError()).toBeUndefined();
  });

  it("keeps prior rows and rejects when the post-scan refresh fails", async () => {
    const get = vi.fn()
      .mockResolvedValueOnce({ data: { priority: [{ sender: "ada@example.com", subject: "Existing", unread: false }] } })
      .mockRejectedValueOnce(new Error("private runtime detail"));
    const state = createRuntimeState({
      get,
      readAppleMail: vi.fn().mockResolvedValue({ fetched: 1, appended: 1, deduplicated: 0 }),
    });
    await state.loadMail();

    await expect(state.readAppleMail()).rejects.toThrow("The local Mail snapshot could not be loaded.");
    expect(state.mail()).toEqual([{ sender: "ada@example.com", subject: "Existing", unread: false }]);
    expect(state.mailError()).toBe("The local Mail snapshot could not be loaded.");
  });

  it("keeps a passive snapshot failure visible without rejecting refresh", async () => {
    const state = createRuntimeState({ get: vi.fn().mockRejectedValue(new Error("private runtime detail")) });
    await expect(state.loadMail()).resolves.toBeUndefined();
    expect(state.mail()).toEqual([]);
    expect(state.mailLoaded()).toBe(true);
    expect(state.mailError()).toBe("The local Mail snapshot could not be loaded.");
  });

  it("shares one background sync and never throws on a runtime failure", async () => {
    let finish!: (result: AppleMailScanResult) => void;
    const readAppleMail = vi.fn(() => new Promise<AppleMailScanResult>((resolve) => { finish = resolve; }));
    const get = vi.fn().mockResolvedValue({ data: { priority: [] } });
    const state = createRuntimeState({ get, readAppleMail });

    const first = state.syncMail({ background: true, staleSeconds: 300 });
    const second = state.syncMail({ background: true, staleSeconds: 300 });
    expect(state.mailSyncing()).toBe(true);
    expect(readAppleMail).toHaveBeenCalledOnce();
    finish({ status: "scanned", fetched: 1, appended: 1, deduplicated: 0, updated: 0, removed: 0 });
    await Promise.all([first, second]);

    expect(state.mailSync()?.status).toBe("scanned");
    expect(get).toHaveBeenCalledWith("/api/v1/mail/summary");
    expect(state.mailSyncing()).toBe(false);
  });

  it("turns a background native error into a status without rejecting", async () => {
    const state = createRuntimeState({
      get: vi.fn(),
      readAppleMail: vi.fn().mockRejectedValue(new Error("private runtime detail")),
    });

    await expect(state.syncMail({ background: true })).resolves.toMatchObject({ status: "error" });
    expect(state.mailSync()?.error).toBe("Mail sync could not reach the Heiwa runtime.");
  });
});
