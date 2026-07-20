export type OperatorActor = {
  kind: string;
  id: string;
};

export type OperatorEvent = {
  schema_version: number;
  event_id: string;
  thread_id: string;
  turn_id: string | null;
  run_id: string | null;
  call_id: string | null;
  event_type: string;
  occurred_at: string;
  actor: OperatorActor;
  risk_class: string;
  sensitivity: string;
  parent_event_id: string | null;
  correlation_id: string | null;
  source_refs: string[];
  evidence_refs: string[];
  payload: Record<string, unknown>;
};

export type OperatorEventFrame = {
  type: "event";
  cursor: string;
  event: OperatorEvent;
};

export type OperatorAssistantDeltaFrame = {
  type: "assistant_delta";
  thread_id: string;
  turn_id: string;
  text: string;
};

export type OperatorCaughtUpFrame = {
  type: "caught_up";
  cursor?: string | null;
};

export type OperatorHeartbeatFrame = {
  type: "heartbeat";
};

export type OperatorInvalidCursorFrame = {
  type: "invalid_cursor";
  code: string;
};

export type OperatorTransportErrorFrame = {
  type: "error";
  code: string;
};

export type OperatorFrame =
  | OperatorEventFrame
  | OperatorAssistantDeltaFrame
  | OperatorCaughtUpFrame
  | OperatorHeartbeatFrame
  | OperatorInvalidCursorFrame
  | OperatorTransportErrorFrame;

export type OperatorHistoryEvent = {
  cursor: string;
  event: OperatorEvent;
};

export type OperatorHistoryResponse = {
  ok: boolean;
  data: {
    events: OperatorHistoryEvent[];
    next_cursor: string | null;
    skipped_lines: number;
  };
};

export type OperatorThreadSummary = {
  thread_id: string;
  title?: string | null;
  status?: string;
  last_cursor?: string | null;
};

export type OperatorRoutePolicy = {
  mode: "auto";
};

export type OperatorTurnSubmission = {
  client_request_id: string;
  prompt: string;
  route_policy: OperatorRoutePolicy;
};

export type OperatorTurnSubmissionResponse = {
  ok: boolean;
  data: {
    thread_id: string;
    turn_id: string;
    cursor: string;
    duplicate: boolean;
    stream_url: string;
  };
};

export type OperatorMessage = {
  id: string;
  role: "user" | "assistant";
  threadId: string;
  turnId?: string;
  body: string;
  occurredAt: string;
  provider?: string;
  model?: string;
  receiptRef?: string;
};

export type OperatorTurn = {
  turnId: string;
  threadId?: string;
  status: string;
  cancelRequested: boolean;
  prompt?: string;
  clientRequestId?: string;
  routePolicy?: Record<string, unknown>;
  startedAt?: string;
  updatedAt?: string;
};

export type OperatorProjection = {
  key: string;
  eventId: string;
  eventType: string;
  threadId: string;
  turnId?: string;
  callId?: string;
  occurredAt: string;
  payload: Record<string, unknown>;
};

export type OperatorSnapshot = {
  seenEventIds: string[];
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
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function isNonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.length > 0;
}

function isNullableString(value: unknown): value is string | null {
  return value === null || typeof value === "string";
}

function isStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((item) => typeof item === "string");
}

function isJsonValue(value: unknown, seen: WeakSet<object>): boolean {
  if (value === null || typeof value === "string" || typeof value === "boolean") return true;
  if (typeof value === "number") return Number.isFinite(value);
  if (typeof value !== "object") return false;
  if (seen.has(value)) return false;
  seen.add(value);
  const prototype = Object.getPrototypeOf(value);
  if (!Array.isArray(value) && prototype !== Object.prototype && prototype !== null) return false;
  const valid = Array.isArray(value)
    ? value.every((item) => isJsonValue(item, seen))
    : Object.values(value).every((item) => isJsonValue(item, seen));
  seen.delete(value);
  return valid;
}

export function isOperatorEvent(value: unknown): value is OperatorEvent {
  if (!isRecord(value) || !isRecord(value.actor) || !isRecord(value.payload)) return false;
  return Number.isInteger(value.schema_version)
    && Number(value.schema_version) >= 1
    && isNonEmptyString(value.event_id)
    && isNonEmptyString(value.thread_id)
    && isNullableString(value.turn_id)
    && isNullableString(value.run_id)
    && isNullableString(value.call_id)
    && isNonEmptyString(value.event_type)
    && isNonEmptyString(value.occurred_at)
    && isNonEmptyString(value.actor.kind)
    && isNonEmptyString(value.actor.id)
    && isNonEmptyString(value.risk_class)
    && isNonEmptyString(value.sensitivity)
    && isNullableString(value.parent_event_id)
    && isNullableString(value.correlation_id)
    && isStringArray(value.source_refs)
    && isStringArray(value.evidence_refs)
    && isJsonValue(value.payload, new WeakSet());
}

export function isOperatorEventFrame(value: unknown): value is OperatorEventFrame {
  return isRecord(value)
    && value.type === "event"
    && isNonEmptyString(value.cursor)
    && isOperatorEvent(value.event);
}
