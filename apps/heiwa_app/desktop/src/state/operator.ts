import { createSignal, type Accessor } from "solid-js";
import { OperatorClient, type OperatorClientDependencies } from "../operator/client";
import { OperatorStore } from "../operator/store";
import type { OperatorSnapshot } from "../operator/types";
import { apiGet, apiPost, operatorSubscribe } from "../runtime";

/**
 * Solid adapter over the operator seam.
 *
 * `operator/{store,client,types}.ts` are deliberately untouched: they carry
 * the durable-stream contract (cursor recovery, replay, generation guards)
 * and their own test suite. This module is the only bridge between that
 * imperative seam and Solid's reactive graph.
 *
 * Frames are coalesced through a scheduler (one animation frame by default)
 * rather than published per-frame. A streaming turn emits a token event per
 * chunk, and `OperatorStore.snapshot()` deep-clones the whole projection, so
 * publishing on every frame would clone state at token rate. Coalescing keeps
 * that to once per painted frame; Solid handles the DOM diffing that the old
 * hand-rolled `requestAnimationFrame` fast path used to do by hand.
 */

export type OperatorStatus = "idle" | "starting" | "ready" | "submitting" | "error";

export type OperatorState = {
  snapshot: Accessor<OperatorSnapshot>;
  status: Accessor<OperatorStatus>;
  /** True when a turn can be submitted right now. */
  ready: Accessor<boolean>;
  start: (threadId: string) => Promise<void>;
  submit: (prompt: string) => Promise<void>;
};

export type OperatorStateOptions = Partial<
  Pick<OperatorClientDependencies, "get" | "post" | "subscribe" | "randomUUID">
> & {
  /** Defers a publish. Swapped in tests for synchronous flushing. */
  schedule?: (task: () => void) => void;
};

function defaultSchedule(task: () => void): void {
  if (typeof requestAnimationFrame === "function") requestAnimationFrame(task);
  else queueMicrotask(task);
}

export function createOperatorState(options: OperatorStateOptions = {}): OperatorState {
  const store = new OperatorStore();
  const [snapshot, setSnapshot] = createSignal<OperatorSnapshot>(store.snapshot());
  const [status, setStatus] = createSignal<OperatorStatus>("idle");

  const schedule = options.schedule ?? defaultSchedule;
  let client!: OperatorClient;
  let pending = false;

  const publish = (): void => {
    if (pending) return;
    pending = true;
    schedule(() => {
      pending = false;
      setSnapshot(store.snapshot());
      setStatus(client.state().status);
    });
  };

  client = new OperatorClient(store, {
    get: options.get ?? apiGet,
    post: options.post ?? apiPost,
    subscribe: options.subscribe ?? operatorSubscribe,
    randomUUID: options.randomUUID ?? (() => crypto.randomUUID()),
    onChange: publish,
    onError: publish,
  });

  return {
    snapshot,
    status,
    ready: () => status() === "ready",
    start: (threadId) => client.start(threadId),
    submit: async (prompt) => {
      await client.submitTurn(prompt);
      publish();
    },
  };
}
