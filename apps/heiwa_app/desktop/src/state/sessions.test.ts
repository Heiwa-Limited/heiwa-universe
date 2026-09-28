import { describe, expect, it, vi } from "vitest";
import { createSessionState } from "./sessions";

describe("SessionState", () => {
  it("loads a Work conversation outside the catalog and preserves its scope through refresh", async () => {
    const start = vi.fn(async () => undefined);
    const getThread = vi.fn(async () => ({ ok: true, data: { thread: { thread_id: "work-thread", title: "Existing conversation", archived: false, work_ids: ["work-one"] } } }));
    const state = createSessionState({ start, getThread,
      get: async () => ({ ok: true, data: { threads: [], projects: [], truncated: true } }),
    });
    await state.load();
    expect(await state.openWork("work-thread", { workId: "work-one", title: "Ship the fix" })).toBe(true);
    expect(getThread).toHaveBeenCalledWith("/api/v1/operator/threads/work-thread");
    expect(start).toHaveBeenLastCalledWith("work-thread", "work-one");
    await state.refresh();
    await state.load();
    expect(state.selectedWork()).toEqual({ workId: "work-one", title: "Ship the fix" });
    expect(start).toHaveBeenLastCalledWith("work-thread", "work-one");
    await state.select("work-thread");
    expect(state.selectedWork()).toBeUndefined();
    expect(start).toHaveBeenLastCalledWith("work-thread");
  });

  it.each(["new selection", "aborted opening"])("ignores a late Work lookup after %s", async (reason) => {
    let release!: (value: { ok: boolean; data: { thread: { thread_id: string; archived: boolean } } }) => void;
    const pending = new Promise<{ ok: boolean; data: { thread: { thread_id: string; archived: boolean } } }>((resolve) => { release = resolve; });
    const start = vi.fn(async () => undefined);
    const state = createSessionState({ start, getThread: () => pending,
      get: async () => ({ ok: true, data: { threads: [{ thread_id: "ordinary" }], projects: [] } }),
    });
    await state.load();
    const controller = new AbortController();
    const opening = state.openWork("work-thread", { workId: "work-one", title: "Fix" }, controller.signal);
    if (reason === "new selection") await state.select("ordinary");
    else controller.abort();
    release({ ok: true, data: { thread: { thread_id: "work-thread", archived: false } } });
    expect(await opening).toBe(false);
    expect(state.selectedId()).toBe("ordinary");
    expect(state.selectedWork()).toBeUndefined();
    expect(state.threads()).toEqual([{ thread_id: "ordinary" }]);
    expect(start).not.toHaveBeenCalledWith("work-thread", "work-one");
  });

  it.each([
    { thread_id: "wrong-thread", archived: false },
    { thread_id: "work-thread", archived: true },
    { thread_id: "work-thread" },
  ])("refuses unavailable Work conversation metadata %j", async (data) => {
    const start = vi.fn(async () => undefined);
    const state = createSessionState({ start, getThread: async () => ({ ok: true, data: { thread: data } }) });
    await expect(state.openWork("work-thread", { workId: "work-one", title: "Fix" })).rejects.toThrow("unavailable or archived");
    expect(start).not.toHaveBeenCalled();
    expect(state.selectedWork()).toBeUndefined();
  });

  it.each([undefined, [], ["work-other"]])("refuses missing or wrong runtime membership %j before starting observation", async (workIds) => {
    const start = vi.fn(async () => undefined);
    const state = createSessionState({ start, getThread: async () => ({ ok: true, data: { thread: { thread_id: "work-thread", archived: false, work_ids: workIds } } }) });
    await expect(state.openWork("work-thread", { workId: "work-one", title: "Fix" })).rejects.toThrow("could not confirm");
    expect(start).not.toHaveBeenCalled();
    expect(state.selectedId()).toBeUndefined();
  });

  it("keeps drafts separate while sessions switch and moves a session through the runtime service", async () => {
    const start = vi.fn(async () => undefined);
    const post = vi.fn(async (path: string, body: unknown) => {
      if (path.includes("metadata")) return { ok: true, data: { thread: { thread_id: "one", title: "One", project_id: (body as { project_id: string | null }).project_id, archived: false } } };
      return { ok: true, data: {} };
    });
    const state = createSessionState({
      start,
      post,
      get: async () => ({ ok: true, data: { threads: [
        { thread_id: "one", title: "One", project_id: null, archived: false },
        { thread_id: "two", title: "Two", project_id: null, archived: false },
      ], projects: [{ project_id: "project-a", title: "Project A", archived: false }] } }),
    });

    await state.load();
    state.setDraft("first draft");
    await state.select("two");
    state.setDraft("second draft");
    await state.select("one");
    await state.moveThread("one", "project-a");

    expect(state.draft()).toBe("first draft");
    expect(start).toHaveBeenCalledWith("one");
    expect(post).toHaveBeenCalledWith("/api/v1/operator/threads/one/metadata", { project_id: "project-a" });
    expect(state.threads()[0]?.project_id).toBe("project-a");
  });

  it("exposes a catalog failure without creating placeholder sessions", async () => {
    const state = createSessionState({ start: async () => undefined, get: async () => { throw new Error("runtime unavailable"); } });
    await state.load();
    expect(state.threads()).toEqual([]);
    expect(state.error()).toBe("runtime unavailable");
  });

  it("disposes observation when archiving the final selected session", async () => {
    const dispose = vi.fn();
    const state = createSessionState({
      start: async () => undefined,
      dispose,
      get: async () => ({ ok: true, data: { threads: [{ thread_id: "only", archived: false }], projects: [] } }),
      post: async () => ({ ok: true, data: { thread: { thread_id: "only", archived: true } } }),
    });
    await state.load();
    await state.archiveThread("only");
    expect(state.selectedId()).toBeUndefined();
    expect(dispose).toHaveBeenCalledOnce();
  });

  it("retains a remembered selection when a truncated catalog cannot include it", async () => {
    const state = createSessionState({
      initialSelectedId: "older-session",
      start: async () => undefined,
      get: async () => ({ ok: true, data: { threads: [{ thread_id: "recent", archived: false }], projects: [], truncated: true } }),
    });
    await state.load();
    expect(state.selectedId()).toBe("recent");
    expect(state.truncated()).toBe(true);
  });

  it("archives and restores a project without changing its member session ID", async () => {
    const post = vi.fn(async (path: string, body: unknown) => ({ ok: true, data: { project: { project_id: "p", title: "Project", archived: Boolean((body as { archived?: boolean }).archived) } } }));
    const state = createSessionState({
      start: async () => undefined,
      post,
      get: async () => ({ ok: true, data: { threads: [{ thread_id: "member", project_id: "p", archived: false }], projects: [{ project_id: "p", title: "Project", archived: false }] } }),
    });
    await state.load();
    await state.archiveProject("p");
    expect(state.threads()[0]?.thread_id).toBe("member");
    expect(state.projects()[0]?.archived).toBe(true);
    await state.restoreProject("p");
    expect(state.projects()[0]?.archived).toBe(false);
  });

  it("does not clear a next draft after an earlier submission settles", async () => {
    const state = createSessionState({ start: async () => undefined, get: async () => ({ ok: true, data: { threads: [{ thread_id: "a", archived: false }], projects: [] } }) });
    await state.load();
    state.setDraft("next message");
    state.clearDraftIfUnchanged("a", "submitted message");
    expect(state.draft()).toBe("next message");
  });
  it("ignores an older catalog response after a newer refresh", async () => {
    let release!: (value: { ok: boolean; data: { threads: { thread_id: string }[]; projects: [] } }) => void;
    const old = new Promise<{ ok: boolean; data: { threads: { thread_id: string }[]; projects: [] } }>((resolve) => { release = resolve; });
    const get = vi.fn().mockReturnValueOnce(old).mockResolvedValueOnce({ ok: true, data: { threads: [{ thread_id: "new" }], projects: [] } });
    const state = createSessionState({ start: async () => undefined, get });
    const loading = state.load();
    await state.refresh();
    release({ ok: true, data: { threads: [{ thread_id: "old" }], projects: [] } });
    await loading;
    expect(state.threads().map((thread) => thread.thread_id)).toEqual(["new"]);
    expect(state.selectedId()).toBe("new");
  });

  it("does not overwrite an acknowledged rename with an in-flight catalog", async () => {
    let release!: (value: { ok: boolean; data: { threads: { thread_id: string; title: string }[]; projects: [] } }) => void;
    const initial = { ok: true, data: { threads: [{ thread_id: "one", title: "Before" }], projects: [] as [] } };
    const get = vi.fn().mockResolvedValueOnce(initial).mockImplementationOnce(() => new Promise((resolve) => { release = resolve; }));
    const state = createSessionState({ start: async () => undefined, get,
      post: async () => ({ ok: true, data: { thread: { thread_id: "one", title: "After" } } }),
    });
    await state.load();
    const refresh = state.refresh();
    await state.renameThread("one", "After");
    release(initial);
    await refresh;
    expect(state.threads()[0].title).toBe("After");
    expect(state.loading()).toBe(false);
  });

  it("switches observation when a refresh finds the selected session archived elsewhere", async () => {
    const start = vi.fn(async () => undefined);
    const get = vi.fn()
      .mockResolvedValueOnce({ ok: true, data: { threads: [{ thread_id: "one" }, { thread_id: "two" }], projects: [] } })
      .mockResolvedValueOnce({ ok: true, data: { threads: [{ thread_id: "one", archived: true }, { thread_id: "two" }], projects: [] } });
    const state = createSessionState({ start, get });
    await state.load();
    await state.refresh();
    expect(state.selectedId()).toBe("two");
    expect(start).toHaveBeenLastCalledWith("two");
  });

  it("keeps observing a selected session omitted by a bounded refresh", async () => {
    const start = vi.fn(async () => undefined);
    const get = vi.fn()
      .mockResolvedValueOnce({ ok: true, data: { threads: [{ thread_id: "older" }], projects: [] } })
      .mockResolvedValueOnce({ ok: true, data: { threads: [{ thread_id: "recent" }], projects: [], truncated: true } });
    const state = createSessionState({ start, get });
    await state.load();
    await state.refresh();
    expect(state.selectedId()).toBe("older");
    expect(state.threads().map((thread) => thread.thread_id)).toContain("older");
    expect(start).toHaveBeenCalledTimes(1);
  });

});
