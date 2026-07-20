import type {
  OperatorFrame,
  OperatorHistoryResponse,
  OperatorTurnSubmission,
  OperatorTurnSubmissionResponse,
} from "./types";
import { isOperatorEventFrame } from "./types";
import { OperatorStore } from "./store";

class StaleClientOperation extends Error {}

const MAX_INVALID_CURSOR_RECOVERIES = 2;

type SubscriptionRun = {
  generation: number;
  invalidRequested: boolean;
  promise: Promise<void>;
};

export type OperatorClientError =
  | "operator_history_unavailable"
  | "operator_stream_unavailable"
  | "operator_submission_unavailable";

export type OperatorClientState =
  | { status: "idle" | "starting" | "ready" | "submitting"; error: null }
  | { status: "error"; error: OperatorClientError };

export type OperatorClientDependencies = {
  get: (path: string) => Promise<OperatorHistoryResponse>;
  post: (path: string, body: OperatorTurnSubmission) => Promise<OperatorTurnSubmissionResponse>;
  subscribe: (threadId: string, after: string | null, onFrame: (frame: OperatorFrame) => void) => Promise<void>;
  randomUUID: () => string;
  onChange?: () => void;
  onError?: (code: OperatorClientError) => void;
};

export class OperatorClient {
  private threadId: string | null = null;
  private clientState: OperatorClientState = { status: "idle", error: null };
  private recovery: { generation: number; promise: Promise<void> } | null = null;
  private activeSubscription: SubscriptionRun | null = null;
  private generation = 0;
  private invalidCursorRecoveries = 0;

  constructor(
    private readonly store: OperatorStore,
    private readonly dependencies: OperatorClientDependencies,
  ) {}

  async start(threadId: string): Promise<void> {
    const normalized = threadId.trim();
    if (!normalized) throw new Error("thread_id_required");
    const generation = ++this.generation;
    this.invalidCursorRecoveries = 0;
    this.threadId = normalized;
    this.store.resetProjectionForReplay();
    this.clientState = { status: "starting", error: null };
    this.dependencies.onChange?.();
    try {
      const cursor = await this.replayHistory(normalized, generation);
      this.assertCurrent(normalized, generation);
      this.clientState = { status: "ready", error: null };
      this.dependencies.onChange?.();
      this.startSubscription(normalized, cursor, generation);
    } catch (error) {
      if (error instanceof StaleClientOperation) return;
      if (this.isCurrent(normalized, generation)) {
        this.store.resetProjectionForReplay();
        this.dependencies.onChange?.();
        this.reportError("operator_history_unavailable", generation);
      }
    }
  }

  async submitTurn(prompt: string): Promise<OperatorTurnSubmissionResponse> {
    const threadId = this.threadId;
    if (!threadId) throw new Error("operator_client_not_started");
    const generation = this.generation;
    const trimmed = prompt.trim();
    if (!trimmed) throw new Error("prompt_required");
    this.clientState = { status: "submitting", error: null };
    this.dependencies.onChange?.();
    try {
      const response = await this.dependencies.post(
        `/api/v1/operator/threads/${encodeURIComponent(threadId)}/turns`,
        {
          client_request_id: this.dependencies.randomUUID(),
          prompt: trimmed,
          route_policy: { mode: "auto" },
        },
      );
      if (this.isCurrent(threadId, generation)) {
        this.clientState = { status: "ready", error: null };
        this.dependencies.onChange?.();
      }
      return response;
    } catch {
      this.reportError("operator_submission_unavailable", generation);
      throw new Error("operator_submission_unavailable");
    }
  }

  state(): OperatorClientState {
    return { ...this.clientState };
  }

  private async replayHistory(threadId: string, generation: number): Promise<string | null> {
    let after: string | null = null;
    let changed = false;
    for (;;) {
      const suffix = after ? `&after=${encodeURIComponent(after)}` : "";
      const response = await this.dependencies.get(
        `/api/v1/operator/threads/${encodeURIComponent(threadId)}/events?limit=500${suffix}`,
      );
      this.assertCurrent(threadId, generation);
      const events = response?.data?.events;
      const next = response?.data?.next_cursor;
      if (!response?.ok
        || !Array.isArray(events)
        || events.length > 500
        || (next !== null && (typeof next !== "string" || next.length === 0))
        || !Number.isInteger(response?.data?.skipped_lines)
        || Number(response?.data?.skipped_lines) < 0
        || events.some((row) => !isOperatorEventFrame({ type: "event", cursor: row?.cursor, event: row?.event }))) {
        throw new Error("operator_history_invalid");
      }
      if (events.length === 500 && (!next || next === after)) {
        throw new Error("operator_history_stalled");
      }
      for (const row of events) {
        changed = this.store.reduce({ type: "event", cursor: row.cursor, event: row.event }) || changed;
      }
      const lastEventCursor = events.at(-1)?.cursor ?? after;
      const serverCursor = next ?? lastEventCursor;
      if (events.length < 500) {
        if (changed) this.dependencies.onChange?.();
        return serverCursor;
      }
      after = next;
    }
  }

  private startSubscription(threadId: string, after: string | null, generation: number): void {
    const run: SubscriptionRun = {
      generation,
      invalidRequested: false,
      promise: Promise.resolve(),
    };
    run.promise = Promise.resolve().then(() => this.dependencies.subscribe(threadId, after, (frame) => {
      if (!this.isCurrent(threadId, generation)) return;
      if (frame !== null && typeof frame === "object" && frame.type === "invalid_cursor") {
        if (run.invalidRequested) return;
        run.invalidRequested = true;
        this.scheduleRecovery(threadId, generation, run);
        return;
      }
      this.reduceFrame(frame);
    }));
    this.activeSubscription = run;
    void run.promise.catch(() => {
      if (!run.invalidRequested && this.isCurrent(threadId, generation)) {
        this.reportError("operator_stream_unavailable", generation);
      }
    }).finally(() => {
      if (this.activeSubscription === run) this.activeSubscription = null;
    });
  }

  private scheduleRecovery(threadId: string, generation: number, run: SubscriptionRun): void {
    if (this.recovery?.generation === generation) return;
    if (this.invalidCursorRecoveries >= MAX_INVALID_CURSOR_RECOVERIES) {
      this.reportError("operator_history_unavailable", generation);
      return;
    }
    this.invalidCursorRecoveries += 1;
    this.clientState = { status: "starting", error: null };
    this.dependencies.onChange?.();
    let recoveryRecord!: { generation: number; promise: Promise<void> };
    const promise = (async () => {
      await run.promise.catch(() => undefined);
      if (!this.isCurrent(threadId, generation)) return;
      try {
        this.store.resetProjectionForReplay();
        this.dependencies.onChange?.();
        const cursor = await this.replayHistory(threadId, generation);
        this.assertCurrent(threadId, generation);
        this.clientState = { status: "ready", error: null };
        this.dependencies.onChange?.();
        if (this.recovery === recoveryRecord) this.recovery = null;
        this.startSubscription(threadId, cursor, generation);
      } catch (error) {
        if (error instanceof StaleClientOperation) return;
        if (this.isCurrent(threadId, generation)) {
          this.store.resetProjectionForReplay();
          this.dependencies.onChange?.();
          this.reportError("operator_history_unavailable", generation);
        }
      }
    })();
    recoveryRecord = { generation, promise };
    this.recovery = recoveryRecord;
    void promise.finally(() => {
      if (this.recovery?.promise === promise) this.recovery = null;
    });
  }

  private reduceFrame(frame: OperatorFrame): void {
    if (this.store.reduce(frame)) this.dependencies.onChange?.();
  }

  private reportError(code: OperatorClientError, generation = this.generation): void {
    if (generation !== this.generation) return;
    this.clientState = { status: "error", error: code };
    this.dependencies.onError?.(code);
  }

  private isCurrent(threadId: string, generation: number): boolean {
    return this.generation === generation && this.threadId === threadId;
  }

  private assertCurrent(threadId: string, generation: number): void {
    if (!this.isCurrent(threadId, generation)) throw new StaleClientOperation();
  }
}
