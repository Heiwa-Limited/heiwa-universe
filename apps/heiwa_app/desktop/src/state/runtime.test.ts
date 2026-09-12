import { describe, expect, it, vi } from "vitest";
import { createRuntimeState } from "./runtime";

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
});
