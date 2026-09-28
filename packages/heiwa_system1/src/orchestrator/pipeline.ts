/**
 * The pipeline router: one inbound state in, one governed outcome out.
 *
 * The full lifecycle of a request:
 *
 *   state -> speculate (one parallel fan of System 1 judgments)
 *         -> gate      (confidence bands, ambiguity, per-question policy)
 *         -> branch    (dispatch | System 2 | quarantine)
 *
 * ## Fail-closed is the whole safety argument
 *
 * A confidence gate that opens when it cannot read the confidence is not a
 * gate. Three separate conditions therefore force quarantine, and each has
 * its own test:
 *
 *   1. A gating question came back below threshold — the ordinary case.
 *   2. A gating question came back *ambiguous* despite high confidence.
 *   3. A gating question has **no answer at all** — the chunk failed, the
 *      provider was down, or the body violated the schema. Absence of a
 *      judgment is never treated as permission.
 *
 * Informational questions are exempt from (3) as well as from the band vote:
 * losing a sentiment read should not stop a refund from being routed.
 *
 * ## Generation is a separate axis from confidence
 *
 * High confidence means "System 1 knows the answer", not "System 1 can do the
 * work". Anything needing long-form text or code goes to System 2 regardless
 * of how sure the classifier was — that is what `requiresGeneration` is for.
 * Keeping it separate from the confidence bands stops the two reasons for
 * escalation from being confused in telemetry.
 */

import type { System1Client } from "../core/system1/client.ts";
import { type System1Error, system1Error } from "../core/system1/errors.ts";
import type { Question } from "../core/system1/primitives.ts";
import type { TokenUsage } from "../core/system1/schema.ts";
import type { DecodedAnswer } from "../core/system1/types.ts";
import { type BatchGate, gateBatch, type QuestionPolicy } from "./gate.ts";
import { type ChunkFailure, speculate } from "./speculative.ts";

export type PipelineRoute = "dispatched" | "deliberated" | "quarantined";

/** Everything a handler needs to act, with no further round trips. */
export type DecisionContext = {
  readonly state: unknown;
  readonly answers: Readonly<Record<string, DecodedAnswer>>;
  readonly gate: BatchGate;
  readonly failures: readonly ChunkFailure[];
  readonly unanswered: readonly string[];
  readonly latencyMs: number;
  readonly usage: TokenUsage;
};

/** The record handed to the review queue. Same shape, plus why it landed. */
export type QuarantineRecord = DecisionContext & {
  readonly reason: "low_confidence" | "ambiguous" | "incomplete" | "vetoed";
};

export type PipelineOutcome<T> = {
  readonly route: PipelineRoute;
  readonly context: DecisionContext;
} & (
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly error: System1Error }
);

export type PipelineOptions<T> = {
  readonly client: System1Client;
  readonly questions: Readonly<Record<string, Question>>;
  readonly policies?: Readonly<Record<string, QuestionPolicy>>;
  /** Fast path: call a tool or microservice directly. */
  dispatch(ctx: DecisionContext): Promise<T>;
  /** Slow path: a generative agent, a fallback model, or a deliberation loop. */
  system2(ctx: DecisionContext): Promise<T>;
  /** Terminal path: park for human review. Must not throw; failures are logged. */
  quarantine(record: QuarantineRecord): Promise<void>;
  /**
   * Force System 2 regardless of confidence — long-form text, code, anything
   * a typed decision cannot produce.
   */
  requiresGeneration?(ctx: DecisionContext): boolean;
};

export class Pipeline<T> {
  readonly #options: PipelineOptions<T>;

  constructor(options: PipelineOptions<T>) {
    this.#options = options;
  }

  async run(state: unknown): Promise<PipelineOutcome<T>> {
    const started = Date.now();
    const o = this.#options;
    const policies = o.policies ?? {};

    const fan = await speculate(o.client, { state, questions: o.questions });

    // A gating question with no answer is a hard stop. Informational ones are
    // allowed to go missing.
    const unanswered = fan.missing;
    const gatingUnanswered = unanswered.filter(
      (id) => !policies[id]?.informational,
    );

    const gate = gateBatch(fan.answers, { policies });

    const context: DecisionContext = {
      state,
      answers: fan.answers,
      gate,
      failures: fan.failures,
      unanswered,
      latencyMs: Date.now() - started,
      usage: fan.usage,
    };

    if (gatingUnanswered.length > 0) {
      return this.#quarantine(context, "incomplete");
    }

    if (gate.band === "quarantine") {
      // Report the most specific reason present. A veto is the strongest
      // signal for a reviewer: it says a rule fired, not that the model was
      // merely unsure.
      const reasons = new Set(
        gate.blockers.map((id) => gate.decisions[id]?.reason),
      );
      const reason: QuarantineRecord["reason"] = reasons.has("vetoed")
        ? "vetoed"
        : reasons.has("ambiguous")
          ? "ambiguous"
          : "low_confidence";
      return this.#quarantine(context, reason);
    }

    const needsGeneration = this.#safeRequiresGeneration(context);
    const route: PipelineRoute =
      gate.band === "auto" && !needsGeneration ? "dispatched" : "deliberated";

    const handler = route === "dispatched" ? o.dispatch : o.system2;

    try {
      const value = await handler.call(o, context);
      return { route, context, ok: true, value };
    } catch (cause) {
      // A handler blowing up is an execution failure, not a routing failure:
      // the route is reported honestly so telemetry still shows which branch
      // was chosen, with the error attached.
      return {
        route,
        context,
        ok: false,
        error: system1Error(
          "transport",
          `${route === "dispatched" ? "dispatch" : "system2"} handler threw: ${String(cause)}`,
          { cause },
        ),
      };
    }
  }

  /** A predicate that throws must not decide the route; treat it as false. */
  #safeRequiresGeneration(ctx: DecisionContext): boolean {
    if (!this.#options.requiresGeneration) return false;
    try {
      return this.#options.requiresGeneration(ctx);
    } catch {
      return false;
    }
  }

  async #quarantine(
    context: DecisionContext,
    reason: QuarantineRecord["reason"],
  ): Promise<PipelineOutcome<T>> {
    const record: QuarantineRecord = { ...context, reason };
    try {
      await this.#options.quarantine(record);
    } catch {
      // An unreachable review queue must not turn a safe quarantine into an
      // unhandled rejection. The verdict stands either way.
    }
    return {
      route: "quarantined",
      context,
      ok: false,
      error: system1Error(
        "schema_violation",
        `quarantined (${reason}): ${context.gate.blockers.join(", ") || context.unanswered.join(", ") || "no gating answer"}`,
      ),
    };
  }
}
