// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";
import { createRoot } from "solid-js";
import { AppProvider, createAppState } from "../state/app";
import type { OperatorHistoryResponse } from "../operator/types";
import { Conversation } from "./Conversation";

const disposers: Array<() => void> = [];
afterEach(() => {
  cleanup();
  disposers.splice(0).forEach((dispose) => dispose());
});

const saved: OperatorHistoryResponse = {
  ok: true,
  data: {
    events: [{
      cursor: "saved-cursor",
      event: {
        schema_version: 1, event_id: "saved-event", thread_id: "session",
        turn_id: "turn", run_id: null, call_id: null, event_type: "user_message",
        occurred_at: "2026-09-12T00:00:00Z", actor: { kind: "operator", id: "test" },
        risk_class: "low", sensitivity: "local_private", parent_event_id: null,
        correlation_id: null, source_refs: [], evidence_refs: [], payload: { text: "Saved work" },
      },
    }],
    next_cursor: "saved-cursor", skipped_lines: 0,
  },
};

it("offers recovery beside existing messages and reconnects without posting work", async () => {
  let disconnect!: (error: Error) => void;
  const disconnected = new Promise<void>((_resolve, reject) => { disconnect = reject; });
  const get = vi.fn().mockResolvedValue(saved);
  const post = vi.fn();
  const subscribe = vi.fn().mockReturnValueOnce(disconnected)
    .mockImplementation(() => new Promise<void>(() => {}));
  const state = createRoot((dispose) => {
    disposers.push(dispose);
    return createAppState({ operator: { get, post, subscribe, schedule: (task) => task() } });
  });
  await state.operator.start("session");
  render(() => <AppProvider state={state}><Conversation /></AppProvider>);
  expect(screen.getByText("Saved work")).toBeTruthy();

  disconnect(new Error("connection lost"));
  await waitFor(() => expect(screen.getByRole("status", { name: "Connection status" })).toBeTruthy());
  expect(screen.getByText("Saved work")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Reconnect" }));

  await waitFor(() => expect(subscribe).toHaveBeenCalledTimes(2));
  expect(get).toHaveBeenCalledTimes(2);
  expect(screen.queryByRole("status", { name: "Connection status" })).toBeNull();
  expect(screen.getAllByText("Saved work")).toHaveLength(1);
  expect(post).not.toHaveBeenCalled();
  state.operator.dispose();
});

it("shows the actual action, sends scoped approval and stop requests, and waits for durable completion", async () => {
  const base = saved.data.events[0];
  const history: OperatorHistoryResponse = { ...saved, data: { ...saved.data, events: [base,
    { cursor: "tool-cursor", event: { ...base.event, event_id: "tool-start", call_id: "call-tool", event_type: "tool_call_started", payload: { name: "process.run", arguments: { executable: "/bin/echo", args: ["verified"] } } } },
    { cursor: "approval-cursor", event: { ...base.event, event_id: "approval-start", call_id: "call-tool", event_type: "approval_requested", payload: { tool: "process.run", request_id: "req-test", outcome: "pending" } } },
  ] } };
  const post = vi.fn().mockResolvedValue({ ok: true, data: {} });
  const cancel = vi.fn().mockResolvedValue({ ok: true });
  const state = createRoot((dispose) => {
    disposers.push(dispose);
    return createAppState({ operator: { get: vi.fn().mockResolvedValue(history), post: vi.fn(), cancel, subscribe: () => new Promise<void>(() => {}), schedule: (task) => task() }, runtime: { post, get: vi.fn().mockResolvedValue({ ok: true, data: {} }) } });
  });
  await state.operator.start("session");
  render(() => <AppProvider state={state}><Conversation /></AppProvider>);
  expect(screen.getByText("Run workspace command")).toBeTruthy();
  expect(screen.getByText(/\/bin\/echo/)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Approve action" }));
  await waitFor(() => expect(post).toHaveBeenCalledWith("/api/v1/approvals/req-test/approve", {}));
  const beforeStatus = state.operator.snapshot().turns[0].status;
  const stop = screen.getByRole("button", { name: "Stop" });
  fireEvent.click(stop); fireEvent.click(stop);
  await waitFor(() => expect(cancel).toHaveBeenCalledExactlyOnceWith("/api/v1/operator/turns/turn/cancel", {}));
  expect(state.operator.snapshot().turns[0].status).toBe(beforeStatus);
  expect(screen.getByText("Stopping…")).toBeTruthy();
  state.operator.dispose();
});

it("shows saved file facts from the runtime receipt even when model wording differs", async () => {
  const base = saved.data.events[0];
  const result: OperatorHistoryResponse = { ...saved, data: { ...saved.data, events: [base,
    { cursor: "verified", event: { ...base.event, event_id: "tool-done", call_id: "saved-file", event_type: "tool_call_completed", payload: { name: "artifact.write", status: "success", verified_result: { kind: "file", path: "/workspace/actual.txt", sha256: "fixture-hash" }, receipt_id: "receipt-one" } } },
  ] } };
  const state = createRoot((dispose) => { disposers.push(dispose); return createAppState({ operator: { get: vi.fn().mockResolvedValue(result), subscribe: () => new Promise<void>(() => {}), schedule: (task) => task() } }); });
  await state.operator.start("session");
  render(() => <AppProvider state={state}><Conversation /></AppProvider>);
  expect(screen.getByText("File saved and verified")).toBeTruthy();
  expect(screen.getByText("/workspace/actual.txt")).toBeTruthy();
  expect(screen.getByText("SHA256: fixture-hash")).toBeTruthy();
  state.operator.dispose();
});
