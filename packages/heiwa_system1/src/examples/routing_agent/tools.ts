/**
 * The microservices the fast path dispatches to.
 *
 * Deliberately trivial — the interesting part is not what they do but that
 * they are only ever reached after the gate has cleared every security,
 * privacy, and triage judgment. A tool here can assume it was called on a
 * message that System 1 was confident about, that is not a prompt injection,
 * and that does not need a human.
 */

export type ToolResult = {
  readonly tool: string;
  readonly summary: string;
  readonly slaHours: number;
};

export type ToolInput = {
  readonly state: unknown;
  readonly route: string;
  readonly urgency: number;
  readonly sentiment: number;
  readonly containsPii: boolean;
};

/**
 * Urgency 0..3 mapped onto an SLA in hours.
 *
 * A Score is probability-weighted, so 1.5 is a real answer rather than an
 * index — truncating it would discard exactly the nuance the primitive
 * exists to provide. So this interpolates between rungs, and clamps at both
 * ends rather than reading past the ladder.
 */
export function slaHoursFor(urgency: number): number {
  const ladder = [72, 24, 8, 1];
  const top = ladder.length - 1;
  const clamped = Math.min(Math.max(urgency, 0), top);
  const low = Math.floor(clamped);
  const high = Math.min(low + 1, top);
  const t = clamped - low;
  return (
    Math.round(((ladder[low] ?? 72) * (1 - t) + (ladder[high] ?? 1) * t) * 10) /
    10
  );
}

export const TOOLS: Readonly<Record<string, (input: ToolInput) => ToolResult>> =
  {
    billing: (input) => ({
      tool: "refund_service",
      summary: `Opened a billing action${input.containsPii ? " (payload redacted: PII present)" : ""}`,
      slaHours: slaHoursFor(input.urgency),
    }),
    technical: (input) => ({
      tool: "incident_service",
      summary: "Filed a technical incident",
      slaHours: slaHoursFor(input.urgency),
    }),
    account: (input) => ({
      tool: "identity_service",
      summary: "Queued an account change",
      slaHours: slaHoursFor(input.urgency),
    }),
    spam: () => ({
      tool: "spam_sink",
      summary: "Discarded as spam",
      slaHours: 0,
    }),
  };

/**
 * Where an unrecognised route goes.
 *
 * Never reached in normal operation: `decodeResponse` rejects any choice
 * outside the options the question offered, and a test asserts every option
 * has a tool. It exists because the obvious alternative is dangerous.
 * `TOOLS[route] ?? TOOLS.billing` would mean that adding a fifth route and
 * forgetting its tool silently wires it to the refund service. An unknown
 * route is a routing failure, and a routing failure belongs with a human.
 */
function manualQueue(input: ToolInput): ToolResult {
  return {
    tool: "manual_queue",
    summary: `Unrecognised route "${input.route}" — no tool registered; escalated for manual handling`,
    slaHours: slaHoursFor(input.urgency),
  };
}

export function toolFor(route: string): (input: ToolInput) => ToolResult {
  return TOOLS[route] ?? manualQueue;
}
