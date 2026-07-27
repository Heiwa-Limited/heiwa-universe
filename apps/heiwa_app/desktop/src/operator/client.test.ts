import { describe, expect, it, vi } from "vitest";
import { OperatorClient, type OperatorClientDependencies } from "./client";
import { OperatorStore } from "./store";
import type {
  OperatorEvent,
  OperatorEventFrame,
  OperatorFrame,
  OperatorHistoryResponse,
  OperatorTurnSubmissionResponse,
} from "./types";

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
  it("replays a future-schema event diagnostically and subscribes after its cursor", async () => {
    const store = new OperatorStore();
    const future = eventFrame(1);
    future.event.schema_version = 2;
    future.event.event_type = "user_message";
    future.event.payload = { text: "must remain uninterpreted" };
    const subscriptions: Array<{ threadId: string; after: string | null }> = [];
    const client = new OperatorClient(store, dependencies({
      get: vi.fn(async () => history([future], future.cursor)),
      subscribe: vi.fn(async (threadId, after) => {
        subscriptions.push({ threadId, after });
      }),
    }));

    await client.start("team & ops");
    await flushAsyncWork();

    expect(subscriptions).toEqual([{ threadId: "team & ops", after: future.cursor }]);
    expect(store.snapshot().cursor).toBe(future.cursor);
    expect(store.snapshot().messages).toHaveLength(0);
    expect(store.snapshot().compatibility.unsupportedSchemaEvents).toBe(1);
  });

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

  it("fails safely when history contains an event from another thread", async () => {
    const get = vi.fn(async () => history([eventFrame(1, "other thread")], "cursor-1"));
    const subscribe = vi.fn(async () => undefined);
    const store = new OperatorStore();
    const client = new OperatorClient(store, dependencies({ get, subscribe }));

    await client.start("team & ops");

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

  it("fails safely and clears partial replay when pagination cursors cycle", async () => {
    const fullPage = Array.from({ length: 500 }, (_, index) => eventFrame(index + 1));
    const cursors = ["cursor-a", "cursor-b", "cursor-a"];
    let page = 0;
    const get = vi.fn(async () => history(fullPage, cursors[page++]!));
    const subscribe = vi.fn(async () => undefined);
    const store = new OperatorStore();
    const client = new OperatorClient(store, dependencies({ get, subscribe }));

    await client.start("team & ops");

    expect(get).toHaveBeenCalledTimes(3);
    expect(subscribe).not.toHaveBeenCalled();
    expect(store.snapshot().messages).toEqual([]);
    expect(client.state()).toEqual({ status: "error", error: "operator_history_unavailable" });
  });

  it("fails safely at the finite history page budget even with unique cursors", async () => {
    const fullPage = Array.from({ length: 500 }, (_, index) => eventFrame(index + 1));
    let page = 0;
    const get = vi.fn(async () => {
      page += 1;
      return history(fullPage, `unique-page-${page}`);
    });
    const subscribe = vi.fn(async () => undefined);
    const store = new OperatorStore();
    const client = new OperatorClient(store, dependencies({ get, subscribe }));

    await client.start("team & ops");

    expect(get).toHaveBeenCalledTimes(1024);
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

  it("remains submitting when invalid-cursor recovery completes before its POST", async () => {
    const firstSubscription = deferred<void>();
    const callbacks: Array<(frame: OperatorFrame) => void> = [];
    const subscribe = vi.fn((_threadId: string, _after: string | null, onFrame: (frame: OperatorFrame) => void) => {
      callbacks.push(onFrame);
      return callbacks.length === 1 ? firstSubscription.promise : Promise.resolve();
    });
    const postResult = deferred<OperatorTurnSubmissionResponse>();
    const client = new OperatorClient(new OperatorStore(), dependencies({
      post: vi.fn(() => postResult.promise),
      subscribe,
    }));

    await client.start("default");
    await flushAsyncWork();
    const submission = client.submitTurn("survive recovery");
    callbacks[0]!({ type: "invalid_cursor", code: "invalid_cursor" });
    expect(client.state()).toEqual({ status: "starting", error: null });

    firstSubscription.resolve();
    await flushAsyncWork();
    expect(subscribe).toHaveBeenCalledTimes(2);
    expect(client.state()).toEqual({ status: "submitting", error: null });

    postResult.resolve({
      ok: true,
      data: {
        thread_id: "default",
        turn_id: "turn-recovered",
        cursor: "cursor-recovered",
        duplicate: false,
        stream_url: "/ws",
      },
    });
    await submission;

    expect(client.state()).toEqual({ status: "ready", error: null });
  });

  it("does not let late recovery success clear a newer submission error", async () => {
    const recoveryHistory = deferred<OperatorHistoryResponse>();
    let historyCalls = 0;
    const get = vi.fn(() => {
      historyCalls += 1;
      return historyCalls === 1 ? Promise.resolve(history([], null)) : recoveryHistory.promise;
    });
    const firstSubscription = deferred<void>();
    const callbacks: Array<(frame: OperatorFrame) => void> = [];
    const subscribe = vi.fn((_threadId: string, _after: string | null, onFrame: (frame: OperatorFrame) => void) => {
      callbacks.push(onFrame);
      return callbacks.length === 1 ? firstSubscription.promise : Promise.resolve();
    });
    const postResult = deferred<OperatorTurnSubmissionResponse>();
    const client = new OperatorClient(new OperatorStore(), dependencies({
      get,
      post: vi.fn(() => postResult.promise),
      subscribe,
    }));

    await client.start("default");
    await flushAsyncWork();
    const submission = client.submitTurn("fail during recovery");
    callbacks[0]!({ type: "invalid_cursor", code: "invalid_cursor" });
    firstSubscription.resolve();
    await flushAsyncWork();
    expect(get).toHaveBeenCalledTimes(2);
    expect(client.state()).toEqual({ status: "starting", error: null });

    postResult.reject(new Error("submission failed"));
    await expect(submission).rejects.toThrow("operator_submission_unavailable");
    expect(client.state()).toEqual({ status: "error", error: "operator_submission_unavailable" });

    recoveryHistory.resolve(history([], null));
    await flushAsyncWork();

    expect(client.state()).toEqual({ status: "error", error: "operator_submission_unavailable" });
  });

  it("does not start invalid-cursor recovery after a terminal submission error", async () => {
    const subscription = deferred<void>();
    const callbacks: Array<(frame: OperatorFrame) => void> = [];
    const subscribe = vi.fn((_threadId: string, _after: string | null, onFrame: (frame: OperatorFrame) => void) => {
      callbacks.push(onFrame);
      return subscription.promise;
    });
    const postResult = deferred<OperatorTurnSubmissionResponse>();
    const get = vi.fn(async () => history([], null));
    const client = new OperatorClient(new OperatorStore(), dependencies({
      get,
      post: vi.fn(() => postResult.promise),
      subscribe,
    }));

    await client.start("default");
    await flushAsyncWork();
    const submission = client.submitTurn("fail before recovery");
    postResult.reject(new Error("submission failed"));
    await expect(submission).rejects.toThrow("operator_submission_unavailable");

    callbacks[0]!({ type: "invalid_cursor", code: "invalid_cursor" });
    await flushAsyncWork();

    expect(get).toHaveBeenCalledOnce();
    expect(client.state()).toEqual({ status: "error", error: "operator_submission_unavailable" });
    subscription.resolve();
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

    const originalStart = client.start("team & ops");
    await originalStart;
    await flushAsyncWork();

    expect(subscribe).toHaveBeenCalledTimes(3);
    expect(get).toHaveBeenCalledTimes(3);
    expect(client.state()).toEqual({ status: "error", error: "operator_history_unavailable" });
    expect(onError).toHaveBeenLastCalledWith("operator_history_unavailable");

    await flushAsyncWork();
    expect(subscribe).toHaveBeenCalledTimes(3);
    expect(get).toHaveBeenCalledTimes(3);

    expect(client.start(" team & ops ")).toBe(originalStart);
    await flushAsyncWork();
    expect(subscribe).toHaveBeenCalledTimes(3);
    expect(get).toHaveBeenCalledTimes(3);
    expect(client.state()).toEqual({ status: "error", error: "operator_history_unavailable" });
  });

  it("coalesces concurrent and repeated same-thread starts onto one promise", async () => {
    const pendingHistory = deferred<OperatorHistoryResponse>();
    const get = vi.fn(async () => pendingHistory.promise);
    const subscribe = vi.fn(async () => undefined);
    const store = new OperatorStore();
    const client = new OperatorClient(store, dependencies({ get, subscribe }));

    const first = client.start(" team & ops ");
    const concurrent = client.start("team & ops");
    expect(concurrent).toBe(first);

    pendingHistory.resolve(history([eventFrame(1)], "cursor-1"));
    await first;
    await flushAsyncWork();

    expect(client.start("team & ops")).toBe(first);
    expect(get).toHaveBeenCalledOnce();
    expect(subscribe).toHaveBeenCalledTimes(1);
    expect(store.snapshot().messages).toHaveLength(1);
    expect(client.state()).toEqual({ status: "ready", error: null });
  });

  it("rejects a different-thread start without mutating the active client", async () => {
    const callbacks: Array<(frame: OperatorFrame) => void> = [];
    const subscription = deferred<void>();
    const subscribe = vi.fn((_threadId: string, _after: string | null, onFrame: (frame: OperatorFrame) => void) => {
      callbacks.push(onFrame);
      return subscription.promise;
    });
    const get = vi.fn(async () => history([], null));
    const store = new OperatorStore();
    const client = new OperatorClient(store, dependencies({ get, subscribe }));

    await client.start("old thread");
    await flushAsyncWork();
    await expect(client.start("new thread")).rejects.toThrow("operator_client_already_started");
    callbacks[0]!(eventFrame(1, "old thread"));

    expect(get).toHaveBeenCalledOnce();
    expect(subscribe).toHaveBeenCalledOnce();
    expect(store.snapshot().messages.map((message) => message.threadId)).toEqual(["old thread"]);
    expect(client.state()).toEqual({ status: "ready", error: null });
    subscription.resolve();
  });

  it("rejects cross-thread live frames and reports accepted frames to onChange", async () => {
    const callbacks: Array<(frame: OperatorFrame) => void> = [];
    const subscribe = vi.fn(async (_threadId: string, _after: string | null, onFrame: (frame: OperatorFrame) => void) => {
      callbacks.push(onFrame);
    });
    const onChange = vi.fn();
    const store = new OperatorStore();
    const client = new OperatorClient(store, dependencies({ subscribe, onChange }));

    await client.start("owned thread");
    await flushAsyncWork();
    onChange.mockClear();

    callbacks[0]!(null as unknown as OperatorFrame);
    callbacks[0]!({ type: "assistant_delta", thread_id: "other thread", turn_id: "turn-1", text: "wrong" });
    callbacks[0]!(eventFrame(1, "other thread"));

    expect(store.snapshot().messages).toEqual([]);
    expect(store.snapshot().transientByTurn).toEqual(Object.create(null));
    expect(onChange).not.toHaveBeenCalled();

    const delta: OperatorFrame = {
      type: "assistant_delta",
      thread_id: "owned thread",
      turn_id: "turn-2",
      text: "accepted",
    };
    const durable = eventFrame(2, "owned thread");
    callbacks[0]!(delta);
    callbacks[0]!(durable);

    expect(store.snapshot().transientByTurn["turn-2"]).toBe("accepted");
    expect(store.snapshot().messages.map((message) => message.threadId)).toEqual(["owned thread"]);
    expect(onChange).toHaveBeenNthCalledWith(1, delta);
    expect(onChange).toHaveBeenNthCalledWith(2, durable);
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

  it("does not let a late submission success clear a newer stream error", async () => {
    const postResult = deferred<OperatorTurnSubmissionResponse>();
    const subscription = deferred<void>();
    const client = new OperatorClient(new OperatorStore(), dependencies({
      post: vi.fn(() => postResult.promise),
      subscribe: vi.fn(() => subscription.promise),
    }));
    await client.start("default");
    await flushAsyncWork();

    const submission = client.submitTurn("run safely");
    expect(client.state()).toEqual({ status: "submitting", error: null });

    subscription.reject(new Error("native stream stopped"));
    await flushAsyncWork();
    expect(client.state()).toEqual({ status: "error", error: "operator_stream_unavailable" });

    postResult.resolve({
      ok: true,
      data: {
        thread_id: "default",
        turn_id: "turn-late",
        cursor: "cursor-late",
        duplicate: false,
        stream_url: "/ws",
      },
    });
    await submission;

    expect(client.state()).toEqual({ status: "error", error: "operator_stream_unavailable" });
  });

  it("rejects new submissions after a terminal stream error", async () => {
    const subscription = deferred<void>();
    const post = vi.fn();
    const client = new OperatorClient(new OperatorStore(), dependencies({
      post,
      subscribe: vi.fn(() => subscription.promise),
    }));
    await client.start("default");
    await flushAsyncWork();

    subscription.reject(new Error("native stream stopped"));
    await flushAsyncWork();

    await expect(client.submitTurn("must not run")).rejects.toThrow("operator_client_not_ready");
    expect(post).not.toHaveBeenCalled();
    expect(client.state()).toEqual({ status: "error", error: "operator_stream_unavailable" });
  });

  it("keeps submitting until every concurrent submission settles", async () => {
    const firstResult = deferred<OperatorTurnSubmissionResponse>();
    const secondResult = deferred<OperatorTurnSubmissionResponse>();
    const post = vi.fn()
      .mockImplementationOnce(() => firstResult.promise)
      .mockImplementationOnce(() => secondResult.promise);
    const client = new OperatorClient(new OperatorStore(), dependencies({ post }));
    await client.start("default");

    const first = client.submitTurn("first");
    const second = client.submitTurn("second");
    firstResult.resolve({
      ok: true,
      data: {
        thread_id: "default",
        turn_id: "turn-first",
        cursor: "cursor-first",
        duplicate: false,
        stream_url: "/ws",
      },
    });
    await first;

    expect(client.state()).toEqual({ status: "submitting", error: null });

    secondResult.resolve({
      ok: true,
      data: {
        thread_id: "default",
        turn_id: "turn-second",
        cursor: "cursor-second",
        duplicate: false,
        stream_url: "/ws",
      },
    });
    await second;

    expect(client.state()).toEqual({ status: "ready", error: null });
  });

  it("returns to ready when concurrent submissions settle out of order", async () => {
    const firstResult = deferred<OperatorTurnSubmissionResponse>();
    const secondResult = deferred<OperatorTurnSubmissionResponse>();
    const post = vi.fn()
      .mockImplementationOnce(() => firstResult.promise)
      .mockImplementationOnce(() => secondResult.promise);
    const client = new OperatorClient(new OperatorStore(), dependencies({ post }));
    await client.start("default");

    const first = client.submitTurn("first");
    const second = client.submitTurn("second");
    secondResult.resolve({
      ok: true,
      data: {
        thread_id: "default",
        turn_id: "turn-second",
        cursor: "cursor-second",
        duplicate: false,
        stream_url: "/ws",
      },
    });
    await second;
    expect(client.state()).toEqual({ status: "submitting", error: null });

    firstResult.resolve({
      ok: true,
      data: {
        thread_id: "default",
        turn_id: "turn-first",
        cursor: "cursor-first",
        duplicate: false,
        stream_url: "/ws",
      },
    });
    await first;

    expect(client.state()).toEqual({ status: "ready", error: null });
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
