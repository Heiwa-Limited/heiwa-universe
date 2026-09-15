// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { AppState } from "./app";
import { connectLegacyEvents } from "./legacy-events";

function setVisibility(state: "visible" | "hidden"): void {
  Object.defineProperty(document, "visibilityState", {
    value: state,
    configurable: true,
  });
  document.dispatchEvent(new Event("visibilitychange"));
}

function harnessApp(): {
  app: AppState;
  loadInbox: ReturnType<typeof vi.fn>;
  loadApprovals: ReturnType<typeof vi.fn>;
  loadHealth: ReturnType<typeof vi.fn>;
} {
  const loadInbox = vi.fn().mockResolvedValue(undefined);
  const loadApprovals = vi.fn().mockResolvedValue(undefined);
  const loadHealth = vi.fn().mockResolvedValue(undefined);
  const app = {
    runtime: { loadInbox, loadApprovals, loadHealth },
  } as unknown as AppState;
  return { app, loadInbox, loadApprovals, loadHealth };
}

// Fake timers do not advance microtasks on their own; give pending promise
// chains (loader calls, the poll's own .catch/.finally) a chance to settle.
async function flush(): Promise<void> {
  for (let tick = 0; tick < 25; tick += 1) await Promise.resolve();
}

beforeEach(() => {
  setVisibility("visible");
  vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "Date"] });
});

afterEach(() => {
  vi.useRealTimers();
});

it("refreshes inbox and approvals immediately, then every interval while visible", async () => {
  const { app, loadInbox, loadApprovals, loadHealth } = harnessApp();
  const dispose = connectLegacyEvents(app, { intervalMs: 1000 });
  await flush();
  expect(loadInbox).toHaveBeenCalledOnce();
  expect(loadApprovals).toHaveBeenCalledOnce();
  expect(loadHealth).toHaveBeenCalledOnce(); // immediate refresh on connect

  await vi.advanceTimersByTimeAsync(1000);
  await flush();
  expect(loadInbox).toHaveBeenCalledTimes(2);
  expect(loadApprovals).toHaveBeenCalledTimes(2);
  expect(loadHealth).toHaveBeenCalledOnce(); // health has its own, much slower cadence

  dispose();
});

it("refreshes health on its own 30s cadence, not the 5s inbox/approvals one", async () => {
  const { app, loadHealth } = harnessApp();
  const dispose = connectLegacyEvents(app);
  await flush();
  expect(loadHealth).toHaveBeenCalledOnce(); // immediate refresh on connect

  await vi.advanceTimersByTimeAsync(5000);
  await flush();
  expect(loadHealth).toHaveBeenCalledOnce(); // not yet due at 5s

  await vi.advanceTimersByTimeAsync(25000); // 30s total since connect
  await flush();
  expect(loadHealth).toHaveBeenCalledTimes(2); // due by 30s

  dispose();
});

it("refreshes health immediately on focus once its 30s gate has elapsed, but not before", async () => {
  const { app, loadHealth } = harnessApp();
  const dispose = connectLegacyEvents(app);
  await flush();
  expect(loadHealth).toHaveBeenCalledOnce();

  // A focus bounce well inside the 30s gate must not trigger another call —
  // the gate applies to focus/visibility-return the same as the timer.
  await vi.advanceTimersByTimeAsync(5000);
  window.dispatchEvent(new Event("focus"));
  await flush();
  expect(loadHealth).toHaveBeenCalledOnce();

  // Once 30s have passed since the last attempt, focus can trigger the next
  // one immediately rather than waiting for the timer.
  await vi.advanceTimersByTimeAsync(25000);
  window.dispatchEvent(new Event("focus"));
  await flush();
  expect(loadHealth).toHaveBeenCalledTimes(2);

  dispose();
});

it("pauses polling while hidden and refreshes immediately on return to visible", async () => {
  const { app, loadInbox } = harnessApp();
  const dispose = connectLegacyEvents(app, { intervalMs: 1000 });
  await flush();
  expect(loadInbox).toHaveBeenCalledOnce();

  setVisibility("hidden");
  await vi.advanceTimersByTimeAsync(5000);
  await flush();
  expect(loadInbox).toHaveBeenCalledOnce(); // no ticks fired real work while hidden

  setVisibility("visible");
  await flush();
  expect(loadInbox).toHaveBeenCalledTimes(2); // immediate refresh on return

  dispose();
});

it("refreshes immediately on window focus", async () => {
  const { app, loadInbox } = harnessApp();
  const dispose = connectLegacyEvents(app, { intervalMs: 1000 });
  await flush();
  expect(loadInbox).toHaveBeenCalledOnce();

  window.dispatchEvent(new Event("focus"));
  await flush();
  expect(loadInbox).toHaveBeenCalledTimes(2);

  dispose();
});

it("never overlaps requests: a slow round trip suppresses the next tick and focus", async () => {
  let resolveInbox: () => void = () => {};
  const loadInbox = vi.fn(
    () =>
      new Promise<void>((resolve) => {
        resolveInbox = resolve;
      }),
  );
  const loadApprovals = vi.fn().mockResolvedValue(undefined);
  const loadHealth = vi.fn().mockResolvedValue(undefined);
  const app = {
    runtime: { loadInbox, loadApprovals, loadHealth },
  } as unknown as AppState;

  const dispose = connectLegacyEvents(app, { intervalMs: 1000 });
  await flush();
  expect(loadInbox).toHaveBeenCalledOnce();

  // The first round trip is still pending; a tick and a focus bounce must
  // not start an overlapping call.
  await vi.advanceTimersByTimeAsync(1000);
  window.dispatchEvent(new Event("focus"));
  await flush();
  expect(loadInbox).toHaveBeenCalledOnce();

  resolveInbox();
  await flush();

  await vi.advanceTimersByTimeAsync(1000);
  await flush();
  expect(loadInbox).toHaveBeenCalledTimes(2);

  dispose();
});

it("keeps last-known state and stays on schedule after a rejected round trip", async () => {
  const loadInbox = vi
    .fn()
    .mockRejectedValueOnce(new Error("offline"))
    .mockResolvedValue(undefined);
  const loadApprovals = vi.fn().mockResolvedValue(undefined);
  const loadHealth = vi.fn().mockResolvedValue(undefined);
  const app = {
    runtime: { loadInbox, loadApprovals, loadHealth },
  } as unknown as AppState;

  const dispose = connectLegacyEvents(app, { intervalMs: 1000 });
  await flush();
  expect(loadInbox).toHaveBeenCalledOnce();

  // A failed round trip is not a reason to retry early or stop polling.
  await vi.advanceTimersByTimeAsync(1000);
  await flush();
  expect(loadInbox).toHaveBeenCalledTimes(2);

  dispose();
});

it("stops the interval and removes listeners on dispose", async () => {
  const { app, loadInbox } = harnessApp();
  const dispose = connectLegacyEvents(app, { intervalMs: 1000 });
  await flush();
  expect(loadInbox).toHaveBeenCalledOnce();

  dispose();
  await vi.advanceTimersByTimeAsync(5000);
  window.dispatchEvent(new Event("focus"));
  setVisibility("hidden");
  setVisibility("visible");
  await flush();
  expect(loadInbox).toHaveBeenCalledOnce();
});
