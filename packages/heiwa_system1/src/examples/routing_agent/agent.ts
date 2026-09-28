/**
 * End-to-end demo: an inbound support message, fully triaged in one round
 * trip, dispatched to a tool only if every safety judgment clears.
 *
 * ## The shape of the thing
 *
 *   payload ──► one Jev request carrying six judgments, in parallel
 *                 route · injection · pii · sentiment · urgency · automatable
 *           ──► gate
 *                 all confident, safe, automatable  ──► tool         (fast)
 *                 unsure, or needs writing          ──► System 2     (slow)
 *                 injection, ambiguous, or missing  ──► review queue (stop)
 *
 * ## Where the policy lives
 *
 * `ROUTING_POLICIES` is the interesting file, not the code path. It encodes
 * three judgements that are genuinely product decisions, not engineering
 * defaults:
 *
 *   - `injection` carries a **veto**, not just a high threshold. A threshold
 *     only asks whether the model was sure; a confident "yes, this IS an
 *     injection" would otherwise pass the gate and be dispatched. The veto
 *     asks what the answer actually says.
 *   - `pii` sits at a 0.9 auto bar rather than 0.95. Remember that a noul's
 *     confidence is derived as |2p - 1|, so 0.95 would demand p <= 0.025 —
 *     near-certainty for a judgment whose only consequence here is a
 *     redaction note. `injection`, whose consequence is money leaving, keeps
 *     the stricter bar.
 *   - `route` carries `minMargin`, because a four-way split where the top two
 *     options are 0.34 and 0.33 is a coin flip no matter how confident the
 *     model sounds — and the four destinations are different teams.
 *   - `sentiment` is informational. Losing a mood read must not stop a
 *     duplicate charge from being refunded.
 */

import {
  type Adapter,
  type CallTelemetry,
  memorySink,
  System1Client,
  typesafeAdapter,
} from "../../core/system1/index.ts";
import {
  type DecisionContext,
  noulThresholds,
  Pipeline,
  type QuarantineRecord,
  type QuestionPolicy,
} from "../../orchestrator/index.ts";
import { INBOUND_QUESTIONS } from "./questions.ts";
import { type ToolResult, toolFor } from "./tools.ts";

export { INBOUND_QUESTIONS } from "./questions.ts";

export const ROUTING_POLICIES: Readonly<Record<string, QuestionPolicy>> = {
  // Four destinations, so a near-tie between the top two is a real risk.
  route: { thresholds: { auto: 0.85, deliberate: 0.5 }, minMargin: 0.2 },
  // A positive injection reading is disqualifying regardless of confidence.
  // The threshold covers "unsure"; the veto covers "sure, and it is bad".
  injection: {
    thresholds: noulThresholds({ autoWithin: 0.05, deliberateWithin: 0.2 }),
    veto: (a) => a.kind === "noul" && a.noul > 0.5,
  },
  // Only gates a redaction note, so "within 0.05 of certain" is the right
  // strictness. Stated as a distance because 0.9-as-confidence does not read
  // like the same strictness as a choice's 0.9.
  pii: {
    thresholds: noulThresholds({ autoWithin: 0.05, deliberateWithin: 0.2 }),
  },
  // Being wrong here costs a trip to System 2, which is a safe failure, so
  // the default bar is right. `requiresGeneration` does the content check.
  automatable: {
    thresholds: noulThresholds({ autoWithin: 0.075, deliberateWithin: 0.25 }),
  },
  urgency: { thresholds: { auto: 0.8, deliberate: 0.5 } },
  // Nice to know; never a reason to stall a refund.
  sentiment: { informational: true },
};

export type InboundPayload = {
  readonly channel: string;
  readonly from: string;
  readonly subject: string;
  readonly message: string;
};

export type RoutingAgent = {
  handle(payload: InboundPayload): Promise<
    {
      route: "dispatched" | "deliberated" | "quarantined";
      context: DecisionContext;
    } & (
      | { ok: true; value: ToolResult }
      | { ok: false; error: { message: string } }
    )
  >;
  readonly dispatched: ToolResult[];
  readonly quarantined: QuarantineRecord[];
  readonly telemetry: { records: CallTelemetry[] };
};

export type RoutingAgentOptions = {
  readonly apiKey?: string;
  readonly baseUrl?: string;
  /** Supply a ready-made adapter (the simulator, a fallback chain, a stub). */
  readonly adapter?: Adapter;
  readonly budgetMs?: number;
};

function answerValue(ctx: DecisionContext, id: string): unknown {
  const a = ctx.answers[id];
  if (!a) return undefined;
  return a.kind === "choice" ? a.choice : a.kind === "score" ? a.score : a.noul;
}

export function buildRoutingAgent(
  options: RoutingAgentOptions = {},
): RoutingAgent {
  const telemetry = memorySink();
  const adapter =
    options.adapter ??
    typesafeAdapter({ apiKey: options.apiKey ?? "", baseUrl: options.baseUrl });

  const client = new System1Client({
    adapter,
    budgetMs: options.budgetMs ?? 500,
    telemetry,
  });

  const dispatched: ToolResult[] = [];
  const quarantined: QuarantineRecord[] = [];

  const pipeline = new Pipeline<ToolResult>({
    client,
    questions: INBOUND_QUESTIONS,
    policies: ROUTING_POLICIES,

    async dispatch(ctx) {
      const route = String(answerValue(ctx, "route") ?? "");
      const result = toolFor(route)({
        state: ctx.state,
        route,
        urgency: Number(answerValue(ctx, "urgency") ?? 0),
        sentiment: Number(answerValue(ctx, "sentiment") ?? 0),
        containsPii: Number(answerValue(ctx, "pii") ?? 0) > 0.5,
      });
      dispatched.push(result);
      return result;
    },

    async system2(ctx) {
      // Stands in for the generative tier. A real host hands the same
      // DecisionContext to a Claude/Ollama call — note it already carries
      // every System 1 judgment, so System 2 never re-derives the triage.
      return {
        tool: "system2_drafter",
        summary: `Escalated to generative tier (route=${String(answerValue(ctx, "route"))}, band=${ctx.gate.band})`,
        slaHours: 4,
      };
    },

    async quarantine(record) {
      quarantined.push(record);
    },

    // Confidence and capability are separate axes: System 1 can be certain
    // about the triage and still be the wrong tool for the job.
    requiresGeneration(ctx) {
      const automatable = ctx.answers.automatable;
      if (!automatable || automatable.kind !== "noul") return true;
      return automatable.noul < 0.5;
    },
  });

  return {
    async handle(payload) {
      const outcome = await pipeline.run(payload);
      return outcome.ok
        ? {
            route: outcome.route,
            context: outcome.context,
            ok: true,
            value: outcome.value,
          }
        : {
            route: outcome.route,
            context: outcome.context,
            ok: false,
            error: outcome.error,
          };
    },
    dispatched,
    quarantined,
    telemetry,
  };
}
