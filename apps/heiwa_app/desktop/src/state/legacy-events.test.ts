import { afterEach, expect, it, vi } from "vitest";
import type { AppState } from "./app";
import { connectLegacyEvents } from "./legacy-events";

class FakeWebSocket {
  static latest: FakeWebSocket | null = null;
  onmessage: ((event: MessageEvent<string>) => void) | null = null;
  onclose: (() => void) | null = null;

  constructor(readonly url: string) {
    FakeWebSocket.latest = this;
  }

  close(): void {}
}

afterEach(() => {
  FakeWebSocket.latest = null;
  vi.unstubAllGlobals();
});

it("refreshes both the inbox and approval queue when a decision changes", async () => {
  vi.stubGlobal("WebSocket", FakeWebSocket);
  const loadInbox = vi.fn().mockResolvedValue(undefined);
  const loadApprovals = vi.fn().mockResolvedValue(undefined);
  const app = {
    runtime: {
      loadInbox,
      loadApprovals,
      loadHealth: vi.fn().mockResolvedValue(undefined),
    },
  } as unknown as AppState;

  const dispose = connectLegacyEvents(app, { url: "ws://127.0.0.1/events" });
  FakeWebSocket.latest?.onmessage?.({
    data: JSON.stringify({ event: "dispatch_request_appeared" }),
  } as MessageEvent<string>);
  await Promise.resolve();

  expect(loadInbox).toHaveBeenCalledOnce();
  expect(loadApprovals).toHaveBeenCalledOnce();
  dispose();
});
