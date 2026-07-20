import { describe, expect, it } from "vitest";
import { OperatorStore } from "./store";
import type { OperatorEvent, OperatorEventFrame, OperatorFrame } from "./types";

let sequence = 0;

function frame(
  id: string,
  eventType: string,
  payload: Record<string, unknown> = {},
  overrides: Partial<OperatorEvent> = {},
): OperatorEventFrame {
  sequence += 1;
  return {
    type: "event",
    cursor: `cursor-${sequence}-${id}`,
    event: {
      schema_version: 1,
      event_id: id,
      thread_id: "default",
      turn_id: "turn-1",
      run_id: null,
      call_id: null,
      event_type: eventType,
      occurred_at: `2026-07-18T00:00:${String(sequence).padStart(2, "0")}Z`,
      actor: { kind: "runtime", id: "operator-turn-runner" },
      risk_class: "low",
      sensitivity: "local_private",
      parent_event_id: null,
      correlation_id: null,
      source_refs: [],
      evidence_refs: [],
      payload,
      ...overrides,
    },
  };
}

describe("OperatorStore", () => {
  it("deduplicates replayed durable events", () => {
    const store = new OperatorStore();
    const user = frame("e1", "user_message", { text: "hello" });

    store.reduce(user);
    store.reduce(user);

    expect(store.snapshot().messages).toHaveLength(1);
    expect(store.snapshot().messages[0]?.body).toBe("hello");
  });

  it("keeps transient deltas disposable and final output durable", () => {
    const store = new OperatorStore();
    store.reduce({ type: "assistant_delta", thread_id: "default", turn_id: "turn-1", text: "hel" });
    store.reduce({ type: "assistant_delta", thread_id: "default", turn_id: "turn-1", text: "lo" });
    store.reduce(frame("e2", "assistant_completed", { text: "hello" }));

    expect(store.snapshot().messages.at(-1)?.body).toBe("hello");
    expect(store.snapshot().transientByTurn["turn-1"]).toBeUndefined();
  });

  it("creates and replaces a durable assistant row with empty completion text", () => {
    const store = new OperatorStore();
    store.reduce({ type: "assistant_delta", thread_id: "default", turn_id: "turn-1", text: "discard me" });
    store.reduce(frame("empty-completion", "assistant_completed", { text: "" }));

    expect(store.snapshot().messages).toEqual([
      expect.objectContaining({ role: "assistant", body: "", turnId: "turn-1" }),
    ]);
    expect(store.snapshot().transientByTurn["turn-1"]).toBeUndefined();

    store.reduce(frame("temporary-completion", "assistant_completed", { text: "temporary" }));
    store.reduce(frame("replacement-empty-completion", "assistant_completed", { text: "" }));
    expect(store.snapshot().messages).toHaveLength(1);
    expect(store.snapshot().messages[0]?.body).toBe("");
  });

  it("ignores late assistant deltas after durable completion", () => {
    const store = new OperatorStore();
    store.reduce(frame("complete-before-delta", "assistant_completed", { text: "final" }));

    store.reduce({ type: "assistant_delta", thread_id: "default", turn_id: "turn-1", text: "late" });

    expect(store.snapshot().messages.at(-1)?.body).toBe("final");
    expect(store.snapshot().transientByTurn["turn-1"]).toBeUndefined();
  });

  it("folds a complete cancellation lifecycle onto one turn", () => {
    const store = new OperatorStore();
    store.reduce(frame("started", "turn_started", { client_request_id: "request-1", route_policy: { mode: "auto" } }));
    store.reduce(frame("user", "user_message", { text: "stop later" }));
    store.reduce(frame("cancel", "turn_cancel_requested", { reason: "operator" }));
    store.reduce(frame("interrupted", "turn_interrupted", { reason: "OPERATOR_CANCELLED" }));

    expect(store.snapshot().turns).toEqual([
      expect.objectContaining({
        turnId: "turn-1",
        prompt: "stop later",
        status: "interrupted",
        cancelRequested: true,
        clientRequestId: "request-1",
      }),
    ]);
  });

  it("advances its cursor only for unseen durable events", () => {
    const store = new OperatorStore();
    const nonDurable: OperatorFrame[] = [
      { type: "assistant_delta", thread_id: "default", turn_id: "turn-1", text: "x" },
      { type: "caught_up" },
      { type: "heartbeat" },
      { type: "invalid_cursor", code: "invalid_cursor" },
      { type: "error", code: "unavailable" },
    ];
    nonDurable.forEach((item) => store.reduce(item));
    expect(store.snapshot().cursor).toBeNull();

    const durable = frame("cursor-event", "future_event", {});
    store.reduce(durable);
    expect(store.snapshot().cursor).toBe(durable.cursor);

    store.reduce({ ...durable, cursor: "duplicate-cursor-must-not-win" });
    expect(store.snapshot().cursor).toBe(durable.cursor);
  });

  it("maps and replaces execution projections by stable domain keys", () => {
    const store = new OperatorStore();
    const events = [
      frame("route-1", "route_planned", { provider: "ollama", model: "qwen", stage: "local" }, { call_id: "call-1" }),
      frame("route-2", "route_completed", { provider: "codex", model: "gpt", stage: "remote" }, { call_id: "call-1" }),
      frame("tool-1", "tool_call_started", { name: "fs.read", status: "running" }, { call_id: "call-1" }),
      frame("tool-2", "tool_call_completed", { name: "fs.read", status: "ok" }, { call_id: "call-1" }),
      frame("approval-1", "approval_requested", { request_id: "approval-1", outcome: "pending" }, { call_id: "call-1" }),
      frame("approval-2", "approval_decided", { request_id: "approval-1", outcome: "approved" }, { call_id: "call-1" }),
      frame("artifact-1", "artifact_created", { artifact_id: "artifact-1", artifact_ref: "local://old" }),
      frame("artifact-2", "artifact_created", { artifact_id: "artifact-1", artifact_ref: "local://new" }),
      frame("receipt-1", "receipt_linked", { receipt_id: "receipt-1", cost_truth: "proxy_estimate" }),
      frame("receipt-2", "receipt_linked", { receipt_id: "receipt-1", cost_truth: "target_only" }),
      frame("blocker-1", "blocker", { blocker_id: "blocker-1", message: "first" }),
      frame("blocker-2", "blocker", { blocker_id: "blocker-1", message: "latest" }),
    ];
    events.forEach((event) => store.reduce(event));

    const snapshot = store.snapshot();
    expect(Object.keys(snapshot.routesByCall)).toEqual(["call-1"]);
    expect(snapshot.routesByCall["call-1"]?.eventType).toBe("route_completed");
    expect(snapshot.routesByCall["call-1"]?.payload.provider).toBe("codex");
    expect(Object.keys(snapshot.toolCalls)).toEqual(["call-1"]);
    expect(snapshot.toolCalls["call-1"]?.payload.status).toBe("ok");
    expect(Object.keys(snapshot.approvals)).toEqual(["approval-1"]);
    expect(snapshot.approvals["approval-1"]?.payload.outcome).toBe("approved");
    expect(snapshot.artifacts["artifact-1"]?.payload.artifact_ref).toBe("local://new");
    expect(snapshot.receipts["receipt-1"]?.payload.cost_truth).toBe("target_only");
    expect(snapshot.blockers["blocker-1"]?.payload.message).toBe("latest");
  });

  it("keeps hostile projection keys in null-prototype records", () => {
    const store = new OperatorStore();
    store.reduce({ type: "assistant_delta", thread_id: "default", turn_id: "__proto__", text: "safe delta" });
    store.reduce(frame("proto-route", "route_planned", { provider: "ollama" }, { call_id: "__proto__" }));
    store.reduce(frame("proto-tool", "tool_call_started", { name: "fs.read" }, { call_id: "__proto__" }));
    store.reduce(frame("proto-approval", "approval_requested", { request_id: "__proto__" }));
    store.reduce(frame("proto-artifact", "artifact_created", { artifact_id: "__proto__" }));
    store.reduce(frame("proto-receipt", "receipt_linked", { receipt_id: "__proto__" }));
    store.reduce(frame("proto-blocker", "blocker", { blocker_id: "__proto__" }, { turn_id: null }));

    const snapshot = store.snapshot();
    const maps = [
      snapshot.routesByCall,
      snapshot.toolCalls,
      snapshot.approvals,
      snapshot.artifacts,
      snapshot.receipts,
      snapshot.blockers,
      snapshot.transientByTurn,
    ];
    maps.forEach((map) => {
      expect(Object.getPrototypeOf(map)).toBeNull();
      expect(Object.hasOwn(map, "__proto__")).toBe(true);
    });
    expect(snapshot.routesByCall["__proto__"]?.payload.provider).toBe("ollama");
    expect(snapshot.transientByTurn["__proto__"]).toBe("safe delta");
  });

  it("enriches a durable assistant message from its turn receipt", () => {
    const store = new OperatorStore();
    store.reduce(frame("assistant", "assistant_completed", { text: "done" }));
    store.reduce(frame("receipt", "receipt_linked", {
      receipt_ref: "receipt://turn-1",
      provider: "ollama",
      model: "qwen3.5:9b",
      cost_truth: "local_zero_cost",
    }));

    expect(store.snapshot().messages).toEqual([
      expect.objectContaining({
        body: "done",
        provider: "ollama",
        model: "qwen3.5:9b",
        receiptRef: "receipt://turn-1",
      }),
    ]);
  });

  it("tolerates unknown events and malformed optional payload fields", () => {
    const store = new OperatorStore();
    const unknown = frame("unknown", "new_event_from_the_future", { text: 42, provider: { nested: true } });
    const malformed = frame("malformed", "assistant_completed", { text: { not: "a string" }, provider: 99 });

    expect(() => store.reduce(unknown)).not.toThrow();
    expect(() => store.reduce(malformed)).not.toThrow();
    expect(store.snapshot().cursor).toBe(malformed.cursor);
    expect(store.snapshot().messages).toHaveLength(0);
  });

  it("rejects malformed frames before cursor or dedup mutation", () => {
    const store = new OperatorStore();
    const valid = frame("not-poisoned", "user_message", { text: "accepted" });
    const malformed: unknown[] = [
      null,
      { type: "event", cursor: valid.cursor, event: null },
      { ...valid, cursor: null },
      { ...valid, event: { ...valid.event, payload: null } },
      { ...valid, event: { ...valid.event, source_refs: null } },
      { ...valid, event: { ...valid.event, evidence_refs: [42] } },
      { ...valid, event: { ...valid.event, actor: null } },
    ];

    malformed.forEach((item) => expect(() => store.reduce(item)).not.toThrow());
    expect(store.snapshot().cursor).toBeNull();
    expect(store.snapshot().seenEventIds).toEqual([]);

    store.reduce(valid);
    expect(store.snapshot().cursor).toBe(valid.cursor);
    expect(store.snapshot().messages[0]?.body).toBe("accepted");
  });

  it("returns snapshots that cannot mutate internal collections or nested payloads", () => {
    const store = new OperatorStore();
    store.reduce(frame("user", "user_message", { text: "immutable" }));
    store.reduce(frame("route", "route_planned", { provider: "ollama", nested: { value: "original" } }, { call_id: "call-1" }));
    const first = store.snapshot();

    first.messages[0]!.body = "corrupted";
    first.turns.push({ turnId: "fake", status: "open", cancelRequested: false });
    (first.routesByCall["call-1"]!.payload.nested as { value: string }).value = "corrupted";
    delete first.routesByCall["call-1"];

    const second = store.snapshot();
    expect(second.messages[0]?.body).toBe("immutable");
    expect(second.turns.some((turn) => turn.turnId === "fake")).toBe(false);
    expect((second.routesByCall["call-1"]?.payload.nested as { value: string }).value).toBe("original");
  });

  it("rebuilds the same snapshot after reset and deterministic replay", () => {
    const store = new OperatorStore();
    const events = [
      frame("started", "turn_started", { client_request_id: "request-1" }),
      frame("user", "user_message", { text: "hello" }),
      frame("route", "route_planned", { provider: "ollama", model: "qwen" }, { call_id: "call-1" }),
      frame("assistant", "assistant_completed", { text: "world" }),
      frame("done", "turn_completed", {}),
    ];
    events.forEach((event) => store.reduce(event));
    const before = store.snapshot();

    store.resetProjectionForReplay();
    expect(store.snapshot().messages).toEqual([]);
    expect(store.snapshot().cursor).toBeNull();
    events.forEach((event) => store.reduce(event));

    expect(store.snapshot()).toEqual(before);
  });
});
