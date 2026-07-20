import { describe, expect, it, vi } from "vitest";
import { OperatorClient, type OperatorClientDependencies } from "./client";
import { OperatorStore } from "./store";
import type { OperatorEvent, OperatorEventFrame, OperatorFrame, OperatorHistoryResponse } from "./types";

function eventFrame(index: number, threadId = "team & ops"): OperatorEventFrame {
  const event: OperatorEvent = {
    schema_version: 1,
    event_id: `event-${index}`,
    thread_id: threadId,
    turn_id: `turn-${index}`,
    run_id: null,
    call_id: null,
    event_type: "user_message",
    occurred_at: "2026-07-18T00:00:00Z",
    actor: { kind: "operator", id: "local-operator" },
    risk_class: "low",
    sensitivity: "local_private",
    parent_event_id: null,
    correlation_id: null,
    source_refs: [],
    evidence_refs: [],
    payload: { text: `message ${index}` },
  };
  return { type: "event", cursor: `cursor-${index}`, event };
}

function history(events: OperatorEventFrame[], nextCursor: string | null): OperatorHistoryResponse {
  return {
    ok: true,
    data: {
      events: events.map(({ cursor, event }) => ({ cursor, event })),
      next_cursor: nextCursor,
      skipped_lines: 0,
    },
  };
}

function dependencies(overrides: Partial<OperatorClientDependencies> = {}): OperatorClientDependencies {
  return {
    get: vi.fn(async () => history([], null)),
    post: vi.fn(async () => ({ ok: true, data: { thread_id: "default", turn_id: "turn-new", cursor: "cursor-new", duplicate: false, stream_url: "/ws" } })),
    subscribe: vi.fn(async () => undefined),
    randomUUID: () => "request-uuid",
    ...overrides,
  };
}

async function flushAsyncWork(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 0));
}

function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void; reject: (error: unknown) => void } {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

describe("OperatorClient", () => {
  it("replays every 500-event history page before subscribing from the final cursor", async () => {
    const firstPage = Array.from({ length: 500 }, (_, index) => eventFrame(index + 1));
    const secondPage = [eventFrame(501)];
    const get = vi.fn(async (path: string) => path.includes("after=cursor-500")
      ? history(secondPage, "cursor-501")
      : history(firstPage, "cursor-500"));
    const subscriptions: Array<{ threadId: string; after: string | null }> = [];
    const subscribe = vi.fn(async (threadId: string, after: string | null, _onFrame: (frame: OperatorFrame) => void) => {
      subscriptions.push({ threadId, after });
    });
    const store = new OperatorStore();
    const client = new OperatorClient(store, dependencies({ get, subscribe }));

    await client.start("team & ops");
    await flushAsyncWork();

    expect(get).toHaveBeenCalledTimes(2);
    expect(get.mock.calls[0]?.[0]).toBe("/api/v1/operator/threads/team%20%26%20ops/events?limit=500");
    expect(get.mock.calls[1]?.[0]).toContain("after=cursor-500");
    expect(store.snapshot().messages).toHaveLength(501);
    expect(subscriptions).toEqual([{ threadId: "team & ops", after: "cursor-501" }]);
  });

  it("does not block initial start on the long-lived native subscription", async () => {
    const subscribe = vi.fn(() => new Promise<void>(() => undefined));
    const client = new OperatorClient(new OperatorStore(), dependencies({ subscribe }));

    await expect(client.start("default")).resolves.toBeUndefined();
    expect(subscribe).toHaveBeenCalledOnce();
  });

  it("fails safely without accepting a full page that has no server cursor", async () => {
    const fullPage = Array.from({ length: 500 }, (_, index) => eventFrame(index + 1));
    const get = vi.fn(async () => history(fullPage, null));
    const subscribe = vi.fn(async () => undefined);
    const store = new OperatorStore();
    const client = new OperatorClient(store, dependencies({ get, subscribe }));

    await client.start("team & ops");
    await flushAsyncWork();

    expect(get).toHaveBeenCalledOnce();
    expect(subscribe).not.toHaveBeenCalled();
    expect(store.snapshot().messages).toEqual([]);
    expect(client.state()).toEqual({ status: "error", error: "operator_history_unavailable" });
  });

  it("fails safely and clears partial replay when a full page cursor does not advance", async () => {
    const firstPage = Array.from({ length: 500 }, (_, index) => eventFrame(index + 1));
    const secondPage = Array.from({ length: 500 }, (_, index) => eventFrame(index + 501));
    const get = vi.fn(async (path: string) => path.includes("after=cursor-500")
      ? history(secondPage, "cursor-500")
      : history(firstPage, "cursor-500"));
    const subscribe = vi.fn(async () => undefined);
    const store = new OperatorStore();
    const client = new OperatorClient(store, dependencies({ get, subscribe }));

    await client.start("team & ops");

    expect(get).toHaveBeenCalledTimes(2);
    expect(subscribe).not.toHaveBeenCalled();
    expect(store.snapshot().messages).toEqual([]);
    expect(client.state()).toEqual({ status: "error", error: "operator_history_unavailable" });
  });

  it("serializes invalid-cursor recovery before one replay and resubscription", async () => {
    let replayCount = 0;
    const get = vi.fn(async () => {
      replayCount += 1;
      return history([eventFrame(replayCount)], `replay-${replayCount}`);
    });
    let active = 0;
    let maxActive = 0;
    const calls: Array<{ after: string | null; onFrame: (frame: OperatorFrame) => void }> = [];
    const subscribe = vi.fn(async (_threadId: string, after: string | null, onFrame: (frame: OperatorFrame) => void) => {
      active += 1;
      maxActive = Math.max(maxActive, active);
      calls.push({ after, onFrame });
      if (calls.length === 1) {
        onFrame({ type: "invalid_cursor", code: "invalid_cursor" });
      }
      active -= 1;
    });
    const store = new OperatorStore();
    const client = new OperatorClient(store, dependencies({ get, subscribe }));

    await client.start("team & ops");
    await flushAsyncWork();

    expect(replayCount).toBe(2);
    expect(calls.map((call) => call.after)).toEqual(["replay-1", "replay-2"]);
    expect(maxActive).toBe(1);
    expect(store.snapshot().messages).toHaveLength(1);
    expect(store.snapshot().messages[0]?.body).toBe("message 2");
  });

  it("waits for the invalidated native invocation to settle before replacement", async () => {
    const firstSettlement = deferred<void>();
    let active = 0;
    let maxActive = 0;
    const callbacks: Array<(frame: OperatorFrame) => void> = [];
    const subscribe = vi.fn((_threadId: string, _after: string | null, onFrame: (frame: OperatorFrame) => void) => {
      callbacks.push(onFrame);
      active += 1;
      maxActive = Math.max(maxActive, active);
      const invocation = callbacks.length === 1 ? firstSettlement.promise : Promise.resolve();
      return invocation.finally(() => { active -= 1; });
    });
    let replay = 0;
    const get = vi.fn(async () => {
      replay += 1;
      return history([eventFrame(replay)], `cursor-${replay}`);
    });
    const client = new OperatorClient(new OperatorStore(), dependencies({ get, subscribe }));

    await client.start("team & ops");
    await flushAsyncWork();
    callbacks[0]!({ type: "invalid_cursor", code: "invalid_cursor" });
    await flushAsyncWork();

    expect(subscribe).toHaveBeenCalledOnce();
    expect(get).toHaveBeenCalledOnce();

    firstSettlement.resolve();
    await flushAsyncWork();

    expect(subscribe).toHaveBeenCalledTimes(2);
    expect(get).toHaveBeenCalledTimes(2);
    expect(maxActive).toBe(1);
  });

  it("recovers when the replacement subscription immediately reports another invalid cursor", async () => {
    let subscriptions = 0;
    const subscribe = vi.fn(async (_threadId: string, _after: string | null, onFrame: (frame: OperatorFrame) => void) => {
      subscriptions += 1;
      if (subscriptions <= 2) onFrame({ type: "invalid_cursor", code: "invalid_cursor" });
    });
    let replays = 0;
    const get = vi.fn(async () => {
      replays += 1;
      return history([eventFrame(replays)], `cursor-${replays}`);
    });
    const store = new OperatorStore();
    const client = new OperatorClient(store, dependencies({ get, subscribe }));

    await client.start("team & ops");
    await flushAsyncWork();

    expect(subscribe).toHaveBeenCalledTimes(3);
    expect(get).toHaveBeenCalledTimes(3);
    expect(store.snapshot().messages).toHaveLength(1);
    expect(store.snapshot().messages[0]?.body).toBe("message 3");
    expect(client.state()).toEqual({ status: "ready", error: null });
  });

  it("stops persistent immediate invalid cursors at a finite generation budget", async () => {
    const onError = vi.fn();
    let subscriptions = 0;
    const subscribe = vi.fn(async (_threadId: string, _after: string | null, onFrame: (frame: OperatorFrame) => void) => {
      subscriptions += 1;
      if (subscriptions <= 4) onFrame({ type: "invalid_cursor", code: "invalid_cursor" });
    });
    let replays = 0;
    const get = vi.fn(async () => {
      replays += 1;
      return history([eventFrame(replays)], `cursor-${replays}`);
    });
    const store = new OperatorStore();
    const client = new OperatorClient(store, dependencies({ get, subscribe, onError }));

    await client.start("team & ops");
    await flushAsyncWork();

    expect(subscribe).toHaveBeenCalledTimes(3);
    expect(get).toHaveBeenCalledTimes(3);
    expect(client.state()).toEqual({ status: "error", error: "operator_history_unavailable" });
    expect(onError).toHaveBeenLastCalledWith("operator_history_unavailable");

    await flushAsyncWork();
    expect(subscribe).toHaveBeenCalledTimes(3);
    expect(get).toHaveBeenCalledTimes(3);

    await client.start("team & ops");
    await flushAsyncWork();
    expect(subscribe).toHaveBeenCalledTimes(5);
    expect(get).toHaveBeenCalledTimes(5);
    expect(client.state()).toEqual({ status: "ready", error: null });
  });

  it("keeps a concurrent stale history start from mutating a switched thread", async () => {
    const oldHistory = deferred<OperatorHistoryResponse>();
    const get = vi.fn(async (path: string) => path.includes("/old%20thread/")
      ? oldHistory.promise
      : history([eventFrame(2, "new thread")], "new-cursor"));
    const subscribe = vi.fn(async () => undefined);
    const store = new OperatorStore();
    const client = new OperatorClient(store, dependencies({ get, subscribe }));

    const oldStart = client.start("old thread");
    await Promise.resolve();
    await client.start("new thread");
    oldHistory.resolve(history([eventFrame(1, "old thread")], "old-cursor"));
    await oldStart;
    await flushAsyncWork();

    expect(store.snapshot().messages.map((message) => message.threadId)).toEqual(["new thread"]);
    expect(subscribe).toHaveBeenCalledTimes(1);
    expect(subscribe).toHaveBeenCalledWith("new thread", "new-cursor", expect.any(Function));
    expect(client.state()).toEqual({ status: "ready", error: null });
  });

  it("ignores frames from a stale subscription after a repeated start", async () => {
    const callbacks: Array<(frame: OperatorFrame) => void> = [];
    const subscriptions = [deferred<void>(), deferred<void>()];
    const subscribe = vi.fn((_threadId: string, _after: string | null, onFrame: (frame: OperatorFrame) => void) => {
      callbacks.push(onFrame);
      return subscriptions[callbacks.length - 1]!.promise;
    });
    const get = vi.fn(async () => history([], null));
    const store = new OperatorStore();
    const client = new OperatorClient(store, dependencies({ get, subscribe }));

    await client.start("old thread");
    await flushAsyncWork();
    await client.start("new thread");
    await flushAsyncWork();
    callbacks[0]!(eventFrame(1, "old thread"));
    callbacks[1]!(eventFrame(2, "new thread"));

    expect(store.snapshot().messages.map((message) => message.threadId)).toEqual(["new thread"]);
    subscriptions.forEach((subscription) => subscription.resolve());
  });

  it("submits a trimmed turn with a random request id and automatic policy without optimistic rows", async () => {
    const post = vi.fn(async () => ({
      ok: true,
      data: { thread_id: "default", turn_id: "turn-new", cursor: "cursor-new", duplicate: false, stream_url: "/ws" },
    }));
    const store = new OperatorStore();
    const client = new OperatorClient(store, dependencies({ post, randomUUID: () => "fresh-request-id" }));
    await client.start("default");

    await client.submitTurn("  do the work  ");

    expect(post).toHaveBeenCalledWith("/api/v1/operator/threads/default/turns", {
      client_request_id: "fresh-request-id",
      prompt: "do the work",
      route_policy: { mode: "auto" },
    });
    expect(store.snapshot().messages).toEqual([]);
  });

  it("rejects blank submission before transport", async () => {
    const post = vi.fn();
    const client = new OperatorClient(new OperatorStore(), dependencies({ post }));
    await client.start("default");

    await expect(client.submitTurn(" \n ")).rejects.toThrow("prompt_required");
    expect(post).not.toHaveBeenCalled();
  });

  it("reports safe client error codes without retaining raw runtime bodies", async () => {
    const onError = vi.fn();
    const get = vi.fn(async () => {
      throw { kind: "Http", detail: { status: 500, body: "<script>token-secret</script>" } };
    });
    const client = new OperatorClient(new OperatorStore(), dependencies({ get, onError }));

    await client.start("default");

    expect(client.state()).toEqual({ status: "error", error: "operator_history_unavailable" });
    expect(onError).toHaveBeenCalledWith("operator_history_unavailable");
    expect(JSON.stringify(client.state())).not.toContain("token-secret");
  });

  it("observes native subscription rejection as a safe error without leaking its body", async () => {
    const onError = vi.fn();
    const subscribe = vi.fn(async () => {
      throw { kind: "Http", detail: { status: 503, body: "<script>stream-token-secret</script>" } };
    });
    const client = new OperatorClient(new OperatorStore(), dependencies({ subscribe, onError }));

    await client.start("default");
    await flushAsyncWork();

    expect(client.state()).toEqual({ status: "error", error: "operator_stream_unavailable" });
    expect(onError).toHaveBeenCalledWith("operator_stream_unavailable");
    expect(JSON.stringify(client.state())).not.toContain("stream-token-secret");
  });
});
