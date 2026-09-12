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
