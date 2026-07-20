import type {
  OperatorEvent,
  OperatorFrame,
  OperatorMessage,
  OperatorProjection,
  OperatorSnapshot,
  OperatorTurn,
} from "./types";
import { isOperatorEventFrame } from "./types";

type MutableState = {
  seenEventIds: Set<string>;
  cursor: string | null;
  messages: OperatorMessage[];
  turns: OperatorTurn[];
  routesByCall: Record<string, OperatorProjection>;
  toolCalls: Record<string, OperatorProjection>;
  approvals: Record<string, OperatorProjection>;
  artifacts: Record<string, OperatorProjection>;
  receipts: Record<string, OperatorProjection>;
  blockers: Record<string, OperatorProjection>;
  transientByTurn: Record<string, string>;
  finalizedAssistantTurns: Set<string>;
};

function nullRecord<T>(): Record<string, T> {
  return Object.create(null) as Record<string, T>;
}

function emptyState(): MutableState {
  return {
    seenEventIds: new Set(),
    cursor: null,
    messages: [],
    turns: [],
    routesByCall: nullRecord(),
    toolCalls: nullRecord(),
    approvals: nullRecord(),
    artifacts: nullRecord(),
    receipts: nullRecord(),
    blockers: nullRecord(),
    transientByTurn: nullRecord(),
    finalizedAssistantTurns: new Set(),
  };
}

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}

function objectValue(value: unknown): Record<string, unknown> | undefined {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : undefined;
}

function clone<T>(value: T): T {
  return structuredClone(value);
}

function cloneRecord<T>(value: Record<string, T>): Record<string, T> {
  const result = nullRecord<T>();
  for (const [key, entry] of Object.entries(value)) result[key] = clone(entry);
  return result;
}

function projection(event: OperatorEvent, key: string): OperatorProjection {
  return {
    key,
    eventId: event.event_id,
    eventType: event.event_type,
    threadId: event.thread_id,
    ...(event.turn_id ? { turnId: event.turn_id } : {}),
    ...(event.call_id ? { callId: event.call_id } : {}),
    occurredAt: event.occurred_at,
    payload: clone(event.payload),
  };
}

export class OperatorStore {
  private state: MutableState = emptyState();

  reduce(frame: OperatorFrame | unknown): boolean {
    if (frame === null || typeof frame !== "object" || Array.isArray(frame)) return false;
    const candidate = frame as Record<string, unknown>;
    if (candidate.type === "assistant_delta") {
      if (typeof candidate.thread_id !== "string" || candidate.thread_id.length === 0
        || typeof candidate.turn_id !== "string" || candidate.turn_id.length === 0
        || typeof candidate.text !== "string"
        || this.state.finalizedAssistantTurns.has(candidate.turn_id)) return false;
      this.state.transientByTurn[candidate.turn_id] = (this.state.transientByTurn[candidate.turn_id] ?? "") + candidate.text;
      return true;
    }
    if (!isOperatorEventFrame(frame)) return false;

    const { event } = frame;
    if (!event || typeof event.event_id !== "string" || this.state.seenEventIds.has(event.event_id)) return false;
    this.state.seenEventIds.add(event.event_id);
    this.state.cursor = frame.cursor;
    this.reduceDurable(event);
    return true;
  }

  resetProjectionForReplay(): void {
    this.state = emptyState();
  }

  snapshot(): OperatorSnapshot {
    return {
      seenEventIds: [...this.state.seenEventIds],
      cursor: this.state.cursor,
      messages: clone(this.state.messages),
      turns: clone(this.state.turns),
      routesByCall: cloneRecord(this.state.routesByCall),
      toolCalls: cloneRecord(this.state.toolCalls),
      approvals: cloneRecord(this.state.approvals),
      artifacts: cloneRecord(this.state.artifacts),
      receipts: cloneRecord(this.state.receipts),
      blockers: cloneRecord(this.state.blockers),
      transientByTurn: cloneRecord(this.state.transientByTurn),
    };
  }

  private reduceDurable(event: OperatorEvent): void {
    const turnId = stringValue(event.turn_id);
    switch (event.event_type) {
      case "turn_started":
        if (turnId) {
          this.patchTurn(event, {
            status: "running",
            cancelRequested: false,
            clientRequestId: stringValue(event.payload.client_request_id),
            routePolicy: objectValue(event.payload.route_policy),
            startedAt: event.occurred_at,
          });
        }
        break;
      case "user_message": {
        const text = stringValue(event.payload.text);
        if (!text) break;
        this.state.messages.push({
          id: event.event_id,
          role: "user",
          threadId: event.thread_id,
          ...(turnId ? { turnId } : {}),
          body: text,
          occurredAt: event.occurred_at,
        });
        if (turnId) this.patchTurn(event, { prompt: text });
        break;
      }
      case "assistant_completed": {
        const text = stringValue(event.payload.text);
        if (turnId) {
          delete this.state.transientByTurn[turnId];
          this.state.finalizedAssistantTurns.add(turnId);
        }
        if (text === undefined) break;
        const message: OperatorMessage = {
          id: event.event_id,
          role: "assistant",
          threadId: event.thread_id,
          ...(turnId ? { turnId } : {}),
          body: text,
          occurredAt: event.occurred_at,
          ...(stringValue(event.payload.provider) ? { provider: stringValue(event.payload.provider) } : {}),
          ...(stringValue(event.payload.model) ? { model: stringValue(event.payload.model) } : {}),
        };
        const existing = turnId
          ? this.state.messages.findIndex((row) => row.role === "assistant" && row.turnId === turnId)
          : -1;
        if (existing >= 0) this.state.messages[existing] = message;
        else this.state.messages.push(message);
        break;
      }
      case "turn_cancel_requested":
        if (turnId) this.patchTurn(event, { status: "cancelling", cancelRequested: true });
        break;
      case "turn_completed":
        if (turnId) {
          this.patchTurn(event, { status: "completed" });
          this.state.finalizedAssistantTurns.add(turnId);
          delete this.state.transientByTurn[turnId];
        }
        break;
      case "turn_interrupted":
        if (turnId) {
          this.patchTurn(event, { status: "interrupted" });
          this.state.finalizedAssistantTurns.add(turnId);
          delete this.state.transientByTurn[turnId];
        }
        break;
      case "route_planned":
      case "route_attempted":
      case "route_completed":
      case "route_failed": {
        const key = stringValue(event.call_id) ?? event.event_id;
        this.state.routesByCall[key] = projection(event, key);
        break;
      }
      case "tool_call_started":
      case "tool_call_completed": {
        const key = stringValue(event.call_id) ?? event.event_id;
        this.state.toolCalls[key] = projection(event, key);
        break;
      }
      case "approval_requested":
      case "approval_decided": {
        const key = stringValue(event.payload.request_id) ?? stringValue(event.call_id) ?? event.event_id;
        this.state.approvals[key] = projection(event, key);
        break;
      }
      case "artifact_created": {
        const key = stringValue(event.payload.artifact_id) ?? stringValue(event.payload.artifact_ref) ?? event.event_id;
        this.state.artifacts[key] = projection(event, key);
        break;
      }
      case "receipt_linked": {
        const key = stringValue(event.payload.receipt_id) ?? stringValue(event.payload.receipt_ref) ?? event.event_id;
        this.state.receipts[key] = projection(event, key);
        this.enrichAssistantFromReceipt(event);
        break;
      }
      case "blocker": {
        const key = stringValue(event.payload.blocker_id) ?? turnId ?? event.event_id;
        this.state.blockers[key] = projection(event, key);
        if (turnId) {
          this.patchTurn(event, { status: "blocked" });
          this.state.finalizedAssistantTurns.add(turnId);
          delete this.state.transientByTurn[turnId];
        }
        break;
      }
      default:
        break;
    }
  }

  private patchTurn(event: OperatorEvent, patch: Partial<OperatorTurn>): void {
    const turnId = stringValue(event.turn_id);
    if (!turnId) return;
    const index = this.state.turns.findIndex((turn) => turn.turnId === turnId);
    const previous = index >= 0 ? this.state.turns[index] : undefined;
    const next: OperatorTurn = {
      turnId,
      threadId: event.thread_id,
      status: "open",
      cancelRequested: false,
      ...previous,
      ...patch,
      updatedAt: event.occurred_at,
    };
    if (index >= 0) this.state.turns[index] = next;
    else this.state.turns.push(next);
  }

  private enrichAssistantFromReceipt(event: OperatorEvent): void {
    const turnId = stringValue(event.turn_id);
    if (!turnId) return;
    const index = this.state.messages.findIndex((message) => message.role === "assistant" && message.turnId === turnId);
    if (index < 0) return;
    const message = this.state.messages[index];
    this.state.messages[index] = {
      ...message,
      ...(stringValue(event.payload.provider) ? { provider: stringValue(event.payload.provider) } : {}),
      ...(stringValue(event.payload.model) ? { model: stringValue(event.payload.model) } : {}),
      ...(stringValue(event.payload.receipt_ref) ? { receiptRef: stringValue(event.payload.receipt_ref) } : {}),
    };
  }
}
