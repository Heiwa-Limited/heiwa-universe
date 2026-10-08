/**
 * Threshold calibration.
 *
 * Shipping a gate with hand-picked thresholds is shipping a guess. 0.85 is a
 * plausible number, not a measured one, and the right value is a property of
 * *your* questions, *your* model, and *your* tolerance for a wrong
 * auto-dispatch — not of this package.
 *
 * This turns that guess into a measurement. Given a corpus of states you
 * have labelled with ground truth, it sweeps candidate thresholds and
 * reports, at each one, the quantities you actually trade off:
 *
 *   autoRate        throughput — the share taking the cheap fast path
 *   falseAutoRate   DANGER — the share auto-dispatched *and wrong*
 *   autoPrecision   how right the fast path was, when it fired
 *   deliberateRate  System 2 spend
 *   quarantineRate  human review load
 *
 * ## The asymmetry that drives the whole thing
 *
 * These costs are not comparable. A quarantine costs a human a minute. A
 * false auto-dispatch issues a refund to an attacker, routes a security
 * incident to the billing queue, or emails the wrong customer. So this does
 * not maximise accuracy, and it does not balance precision against recall.
 *
 * It maximises throughput **subject to a hard ceiling on false
 * auto-dispatch**: pick the most permissive bar whose `falseAutoRate` still
 * fits the budget you declared. If no bar fits, it returns `undefined`
 * rather than a best-effort suggestion — "there is no safe operating point
 * for this question on this corpus" is a real and important answer, and
 * usually means the criteria need separating or the question splitting.
 *
 * ## Getting a corpus
 *
 * Run the fan over historical states, record the answers, and label what the
 * right outcome was. A hundred samples per question is enough to move from
 * "guess" to "measured"; a thousand is enough to trust the tails.
 */

import { confidenceOf } from "../core/system1/confidence.ts";
import type { DecodedAnswer } from "../core/system1/types.ts";
import {
  type GateBand,
  type GateThresholds,
  gateAnswer,
  type QuestionPolicy,
} from "./gate.ts";

/** Ground truth for one question: an option key, a level, or a boolean. */
export type TruthValue = string | number | boolean;

export type LabelledSample = {
  readonly id: string;
  /** What System 1 actually returned for this state. */
  readonly answers: Readonly<Record<string, DecodedAnswer>>;
  /** What the answer should have been, per question id. */
  readonly truth: Readonly<Record<string, TruthValue>>;
};

export type CalibrationMetrics = {
  /** Samples that had both an answer and a truth label for this question. */
  readonly evaluated: number;
  readonly autoRate: number;
  readonly deliberateRate: number;
  readonly quarantineRate: number;
  /** Auto-dispatched AND wrong. The number to drive to zero. */
  readonly falseAutoCount: number;
  readonly falseAutoRate: number;
  /** Accuracy within the auto-dispatched slice; undefined if none fired. */
  readonly autoPrecision: number | undefined;
  /** Correct answers that were held back — the cost of being careful. */
  readonly missedAutoCount: number;
};

/**
 * How much the confidence signal actually varies across the corpus.
 *
 * A sweep is only meaningful if confidence discriminates. Two situations
 * make it meaningless, and both look like a normal table unless you check:
 *
 *  - A capped adapter. `structuredLlmAdapter` clamps confidence to a
 *    ceiling, so every sufficiently-confident answer reports the SAME
 *    number. The sweep then shows a cliff at the ceiling and nothing else —
 *    not because the threshold is right, but because the signal is gone.
 *  - A model that reports one confidence for everything.
 *
 * `distinctValues === 1` means no threshold can separate anything.
 */
export type ConfidenceSpread = {
  readonly distinctValues: number;
  readonly min: number;
  readonly max: number;
  /** Share of answers sitting on the single most common value. */
  readonly modeShare: number;
  /** True when the signal is too flat for a sweep to mean anything. */
  readonly degenerate: boolean;
};

export type SweepPoint = {
  readonly thresholds: GateThresholds;
  readonly metrics: CalibrationMetrics;
};

/**
 * Did the answer match ground truth?
 *
 * A score is a probability-weighted float, so exact equality is the wrong
 * test — `tolerance` (default 0.5, i.e. "rounds to the right level") decides.
 * A noul is judged by which side of 0.5 it falls on.
 */
export function isAnswerCorrect(
  answer: DecodedAnswer,
  truth: TruthValue,
  tolerance = 0.5,
): boolean {
  switch (answer.kind) {
    case "choice":
      return answer.choice === truth;
    case "score":
      return (
        typeof truth === "number" && Math.abs(answer.score - truth) <= tolerance
      );
    case "noul":
      return answer.noul > 0.5 === Boolean(truth);
  }
}

/**
 * Measure how much the confidence signal varies for one question.
 * Call this before trusting a sweep.
 */
export function confidenceSpread(
  samples: readonly LabelledSample[],
  questionId: string,
  degenerateModeShare = 0.9,
): ConfidenceSpread {
  const values: number[] = [];
  for (const sample of samples) {
    const answer = sample.answers[questionId];
    if (answer) values.push(confidenceOf(answer));
  }

  if (values.length === 0) {
    return {
      distinctValues: 0,
      min: 0,
      max: 0,
      modeShare: 0,
      degenerate: true,
    };
  }

  const counts = new Map<number, number>();
  for (const v of values) {
    // Round before bucketing: float noise would otherwise make identical
    // capped values look distinct and hide the very problem we are checking.
    const key = Math.round(v * 1e6) / 1e6;
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  const modeCount = Math.max(...counts.values());

  return {
    distinctValues: counts.size,
    min: Math.min(...values),
    max: Math.max(...values),
    modeShare: modeCount / values.length,
    degenerate:
      counts.size <= 1 || modeCount / values.length >= degenerateModeShare,
  };
}

export function metricsAt(
  samples: readonly LabelledSample[],
  questionId: string,
  thresholds: GateThresholds,
  policy: Omit<QuestionPolicy, "thresholds"> = {},
  tolerance = 0.5,
): CalibrationMetrics {
  let evaluated = 0;
  const bands: Record<GateBand, number> = {
    auto: 0,
    deliberate: 0,
    quarantine: 0,
  };
  let falseAutoCount = 0;
  let autoCorrect = 0;
  let missedAutoCount = 0;

  for (const sample of samples) {
    const answer = sample.answers[questionId];
    const truth = sample.truth[questionId];
    // A sample missing either half tells us nothing about this threshold.
    if (!answer || truth === undefined) continue;
    evaluated++;

    const band = gateAnswer(answer, { ...policy, thresholds }).band;
    bands[band]++;

    const correct = isAnswerCorrect(answer, truth, tolerance);
    if (band === "auto") {
      if (correct) autoCorrect++;
      else falseAutoCount++;
    } else if (correct) {
      missedAutoCount++;
    }
  }

  const rate = (n: number) => (evaluated === 0 ? 0 : n / evaluated);

  return {
    evaluated,
    autoRate: rate(bands.auto),
    deliberateRate: rate(bands.deliberate),
    quarantineRate: rate(bands.quarantine),
    falseAutoCount,
    falseAutoRate: rate(falseAutoCount),
    // Undefined, not 1: "nothing was dispatched" is not "everything
    // dispatched was right". Reporting 1 there would make the safest
    // possible threshold look like the best performing one.
    autoPrecision: bands.auto === 0 ? undefined : autoCorrect / bands.auto,
    missedAutoCount,
  };
}

export type SweepOptions = {
  /** Candidate auto thresholds, evenly spaced over [0, 1]. */
  readonly steps?: number;
  /** Held fixed while the auto bar moves. */
  readonly deliberate?: number;
  readonly policy?: Omit<QuestionPolicy, "thresholds">;
  readonly tolerance?: number;
};

export function sweep(
  samples: readonly LabelledSample[],
  questionId: string,
  options: SweepOptions = {},
): SweepPoint[] {
  const steps = Math.max(2, options.steps ?? 21);
  const points: SweepPoint[] = [];

  for (let i = 0; i < steps; i++) {
    const auto = i / (steps - 1);
    const thresholds: GateThresholds = {
      auto,
      // The deliberate bar cannot sit above the auto bar or the bands invert.
      deliberate: Math.min(options.deliberate ?? 0.5, auto),
    };
    points.push({
      thresholds,
      metrics: metricsAt(
        samples,
        questionId,
        thresholds,
        options.policy,
        options.tolerance,
      ),
    });
  }
  return points;
}

export type RecommendOptions = SweepOptions & {
  /** Hard ceiling on wrong auto-dispatches. Usually 0. */
  readonly maxFalseAutoRate: number;
  /** Reject an operating point that dispatches too little to be worth it. */
  readonly minAutoRate?: number;
};

/**
 * The most permissive threshold that still respects the error budget.
 *
 * Returns `undefined` when no candidate qualifies. That is a finding, not a
 * failure: it means confidence does not separate right from wrong for this
 * question on this corpus, and no threshold will fix that.
 *
 * A bar that dispatches nothing never qualifies, even though it trivially
 * has zero false dispatches. "0 wrong out of 0" is a disabled fast path
 * dressed up as a perfect score.
 *
 * ## Limitation
 *
 * Only the `auto` bar is swept; `deliberate` is held fixed. A full 2-D sweep
 * would also tune the deliberate/quarantine split, which trades System 2
 * spend against human review load. That split does not affect
 * `falseAutoRate` — the number this optimises — so it is left to the
 * operator, who knows what an hour of review costs relative to a System 2
 * call.
 */
export function recommendThresholds(
  samples: readonly LabelledSample[],
  questionId: string,
  options: RecommendOptions,
): SweepPoint | undefined {
  const points = sweep(samples, questionId, options);
  const minAutoRate = options.minAutoRate ?? 0;

  const viable = points.filter(
    (p) =>
      p.metrics.evaluated > 0 &&
      // A bar that never fires trivially has zero false dispatches. That is
      // a disabled fast path, not an operating point, and returning it would
      // read as success — so it never qualifies.
      p.metrics.autoRate > 0 &&
      p.metrics.falseAutoRate <= options.maxFalseAutoRate + 1e-9 &&
      p.metrics.autoRate >= minAutoRate,
  );
  if (viable.length === 0) return undefined;

  // Among safe points, take the one that dispatches most; break ties toward
  // the higher bar, which keeps the extra margin for free.
  return viable.reduce((best, p) => {
    if (p.metrics.autoRate > best.metrics.autoRate) return p;
    if (
      p.metrics.autoRate === best.metrics.autoRate &&
      p.thresholds.auto > best.thresholds.auto
    ) {
      return p;
    }
    return best;
  });
}

/** Render a sweep as a fixed-width table for a terminal or a report. */
export function formatSweep(points: readonly SweepPoint[]): string {
  const head = "  auto   n     auto%   delib%  quar%   falseAuto  precision";
  const rows = points.map((p) => {
    const m = p.metrics;
    const pct = (n: number) => `${(n * 100).toFixed(1)}%`.padStart(6);
    const prec =
      m.autoPrecision === undefined
        ? "     —"
        : `${(m.autoPrecision * 100).toFixed(1)}%`.padStart(6);
    return [
      p.thresholds.auto.toFixed(2).padStart(6),
      String(m.evaluated).padStart(4),
      pct(m.autoRate),
      pct(m.deliberateRate),
      pct(m.quarantineRate),
      String(m.falseAutoCount).padStart(9),
      prec,
    ].join("  ");
  });
  return [head, ...rows].join("\n");
}
