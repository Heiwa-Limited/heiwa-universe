/**
 * Observability hooks.
 *
 * A confidence-gated system is only operable if you can answer, after the
 * fact: what did System 1 decide, how sure was it, how long did it take, what
 * did it cost, and which branch did the gate pick? Every System 1 call emits
 * one `CallTelemetry` record carrying all of that. The default sink is a
 * no-op, so nothing is logged unless the host wires a sink — this package
 * never writes to stdout or to disk on its own.
 *
 * In Heiwa terms this is Evidence-plane material: a host can forward these
 * records into `~/.heiwa/evidence/` as JSONL without reshaping them.
 */

import type { System1Error } from "./errors.ts";

export type QuestionTelemetry = {
  readonly id: string;
  readonly kind: "choice" | "score" | "noul";
  readonly confidence: number;
  /** "model" for choice/score, "derived" for noul. */
  readonly confidenceSource: "model" | "derived";
};

export type CallTelemetry = {
  readonly adapter: string;
  readonly model: string;
  readonly questionCount: number;
  /** Wall-clock time from request dispatch to decoded result. */
  readonly latencyMs: number;
  readonly budgetMs: number;
  /** True when latency exceeded the configured budget but still returned. */
  readonly overBudget: boolean;
  readonly attempts: number;
  readonly inputTokens: number;
  readonly outputTokens: number;
  readonly questions: readonly QuestionTelemetry[];
  readonly error?: System1Error;
};

export type TelemetrySink = (record: CallTelemetry) => void;

export const noopSink: TelemetrySink = () => {};

/** Collects records in memory. Useful for tests and for a host-side buffer. */
export function memorySink(): TelemetrySink & { records: CallTelemetry[] } {
  const records: CallTelemetry[] = [];
  const sink = ((record: CallTelemetry) => {
    records.push(record);
  }) as TelemetrySink & { records: CallTelemetry[] };
  sink.records = records;
  return sink;
}
