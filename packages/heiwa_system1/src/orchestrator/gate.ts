/**
 * The confidence gate.
 *
 * This is the whole point of a two-tier system: a fast, cheap judgment is only
 * safe to act on when the model is sure. The gate turns a calibrated
 * probability into one of three operational verdicts.
 *
 *   auto        confidence >  0.85   dispatch directly to a tool or service
 *   deliberate  0.50 <= c <= 0.85    escalate to System 2 or a fallback agent
 *   quarantine  confidence <  0.50   park for human review, with telemetry
 *
 * ## Two different ways to be unsure
 *
 * Low confidence is not the only failure mode. Jev returns the full
 * distribution, which lets us catch a second one: an answer can carry high
 * reported confidence while the top two options sit almost on top of each
 * other. "Billing 0.50 / technical 0.48" is a coin flip wearing a confident
 * face. `minMargin` is an opt-in check on that gap, and it quarantines with
 * reason `ambiguous` rather than `low_confidence`, because the two call for
 * different remedies: more evidence versus better-separated criteria.
 *
 * ## Confidence is not safety
 *
 * The bands measure how SURE the model is. They say nothing about whether the
 * answer is acceptable. A confident "yes, this is a prompt injection" scores
 * 0.94 and would otherwise sail straight into the auto lane — certainty about
 * a dangerous answer is the most dangerous case, not the safest. `veto` is
 * the separate, content-based guard: a predicate on the answer value that
 * forces quarantine whatever the confidence. Every hard safety rule belongs
 * there, not in a threshold.
 *
 * A veto predicate that throws is treated as a veto. A guard that cannot run
 * is not a guard that passed.
 *
 * ## A trap: noul thresholds are not choice thresholds
 *
 * Both are [0, 1], but they are not the same scale. A choice's confidence is
 * model-reported. A noul's is derived as |2p - 1|, so demanding 0.95 from a
 * noul means demanding p <= 0.025 or p >= 0.975 — far stricter than it looks
 * beside a choice's 0.95. Tune noul thresholds against the probability you
 * actually expect, not by analogy with a choice.
 *
 * ## Conservative aggregation
 *
 * A batch is only as safe as its least safe gating question. `gateBatch`
 * takes the minimum band across all questions that are allowed to gate, and
 * names the blockers, so the caller knows *which* judgment stopped the
 * auto-path rather than just that something did. Questions marked
 * `informational` are still evaluated and reported — they just do not vote.
 */

import type { ConfidenceSource } from "../core/system1/confidence.ts";
import {
  confidenceOf,
  confidenceSourceOf,
} from "../core/system1/confidence.ts";
import type { DecodedAnswer } from "../core/system1/types.ts";

export type GateBand = "auto" | "deliberate" | "quarantine";

export type GateReason =
  | "confident"
  | "uncertain"
  | "low_confidence"
  | "ambiguous"
  | "vetoed";

export type GateThresholds = {
  /** Strictly above this dispatches automatically. */
  readonly auto: number;
  /** At or above this deliberates; below this quarantines. */
  readonly deliberate: number;
};

export const DEFAULT_THRESHOLDS: GateThresholds = {
  auto: 0.85,
  deliberate: 0.5,
};

/**
 * Express noul thresholds in probability distance instead of confidence.
 *
 * Writing `{ auto: 0.9 }` for a noul looks like the choice equivalent but
 * silently means "p must land within 0.05 of certain", because confidence is
 * derived as |2p - 1|. That mismatch is easy to get wrong and hard to see in
 * review. This states the intent directly:
 *
 *     noulThresholds({ autoWithin: 0.05, deliberateWithin: 0.25 })
 *     // "auto-dispatch when p is within 0.05 of 0 or 1;
 *     //  deliberate out to 0.25; quarantine past that."
 *
 * The gate compares with a strict `>`, and the |2p - 1| derivation carries
 * float error (|2 * 0.95 - 1| is 0.9000000000000001, but the neighbouring
 * cases round the other way). So the bar is nudged one epsilon below the
 * exact boundary, which is what "within 0.05" means in words: an answer
 * landing exactly at the stated distance passes.
 *
 * The tolerance lives here rather than in `gateAnswer` on purpose — this is
 * the only place that knows a confidence was derived from a probability.
 * A hand-written `{ auto: 0.85 }` keeps exact `>` semantics.
 */
export function noulThresholds(spec: {
  autoWithin: number;
  deliberateWithin: number;
}): GateThresholds {
  if (spec.deliberateWithin < spec.autoWithin) {
    throw new TypeError(
      `deliberateWithin must be at least autoWithin, got ${spec.deliberateWithin} < ${spec.autoWithin}`,
    );
  }
  const EPS = 1e-9;
  const round = (n: number) => Math.round(n * 1e6) / 1e6;
  return {
    auto: round(1 - 2 * spec.autoWithin) - EPS,
    deliberate: round(1 - 2 * spec.deliberateWithin) - EPS,
  };
}

export type QuestionPolicy = {
  readonly thresholds?: GateThresholds;
  /**
   * Minimum gap between the top two outcomes. 0 (the default) disables the
   * check; set it on decisions where two near-tied options mean different
   * downstream actions.
   */
  readonly minMargin?: number;
  /** Evaluated and reported, but excluded from the batch verdict. */
  readonly informational?: boolean;
  /**
   * Content-based hard stop. Return true to quarantine regardless of
   * confidence. This is where safety rules live — "the injection check came
   * back positive", "the route is `delete_everything`" — because those are
   * facts about the answer, not about how sure the model was.
   */
  veto?(answer: DecodedAnswer): boolean;
};

export type GateDecision = {
  readonly band: GateBand;
  readonly reason: GateReason;
  readonly confidence: number;
  readonly confidenceSource: ConfidenceSource;
  readonly margin: number;
  readonly thresholds: GateThresholds;
};

export type BatchGate = {
  readonly band: GateBand;
  readonly decisions: Readonly<Record<string, GateDecision>>;
  /** Gating questions whose band equals the (worst) overall band. */
  readonly blockers: readonly string[];
};

const BAND_SEVERITY: Record<GateBand, number> = {
  auto: 2,
  deliberate: 1,
  quarantine: 0,
};

/**
 * How separated the top two outcomes are.
 *
 * choice/score: the gap between the two largest probabilities.
 * noul: distance from 0.5, which is the same quantity — a noul's implicit
 * two-outcome distribution is {p, 1 - p}, whose gap is |2p - 1|.
 */
export function ambiguityMargin(answer: DecodedAnswer): number {
  if (answer.kind === "noul") return Math.abs(2 * answer.noul - 1);

  const values =
    answer.kind === "choice"
      ? Object.values(answer.probabilities)
      : Array.from(answer.probabilities);

  if (values.length < 2) return 1;
  const sorted = [...values].sort((a, b) => b - a);
  return (sorted[0] ?? 0) - (sorted[1] ?? 0);
}

export function gateAnswer(
  answer: DecodedAnswer,
  policy: QuestionPolicy = {},
): GateDecision {
  const thresholds = policy.thresholds ?? DEFAULT_THRESHOLDS;
  const confidence = confidenceOf(answer);
  const margin = ambiguityMargin(answer);
  const minMargin = policy.minMargin ?? 0;

  // The veto runs before anything else: it is the only check whose answer
  // does not depend on confidence, and it outranks every other verdict.
  if (policy.veto && vetoed(policy.veto, answer)) {
    return {
      band: "quarantine",
      reason: "vetoed",
      confidence,
      confidenceSource: confidenceSourceOf(answer),
      margin,
      thresholds,
    };
  }

  // Ambiguity is checked next: a near-tie is a harder stop than a soft
  // confidence score, and reporting it as `ambiguous` tells the operator the
  // criteria need separating, not that more evidence is needed.
  if (minMargin > 0 && margin < minMargin) {
    return {
      band: "quarantine",
      reason: "ambiguous",
      confidence,
      confidenceSource: confidenceSourceOf(answer),
      margin,
      thresholds,
    };
  }

  const band: GateBand =
    confidence > thresholds.auto
      ? "auto"
      : confidence >= thresholds.deliberate
        ? "deliberate"
        : "quarantine";

  const reason: GateReason =
    band === "auto"
      ? "confident"
      : band === "deliberate"
        ? "uncertain"
        : "low_confidence";

  return {
    band,
    reason,
    confidence,
    confidenceSource: confidenceSourceOf(answer),
    margin,
    thresholds,
  };
}

/** A guard that throws has not passed. Fail closed. */
function vetoed(
  predicate: (a: DecodedAnswer) => boolean,
  answer: DecodedAnswer,
): boolean {
  try {
    return predicate(answer);
  } catch {
    return true;
  }
}

export function gateBatch(
  answers: Readonly<Record<string, DecodedAnswer>>,
  options: {
    readonly policies?: Readonly<Record<string, QuestionPolicy>>;
  } = {},
): BatchGate {
  const policies = options.policies ?? {};
  const decisions: Record<string, GateDecision> = {};
  let worst: GateBand = "auto";

  for (const [id, answer] of Object.entries(answers)) {
    const policy = policies[id] ?? {};
    const decision = gateAnswer(answer, policy);
    decisions[id] = decision;

    if (
      !policy.informational &&
      BAND_SEVERITY[decision.band] < BAND_SEVERITY[worst]
    ) {
      worst = decision.band;
    }
  }

  const blockers = Object.entries(decisions)
    .filter(([id, d]) => !policies[id]?.informational && d.band === worst)
    .map(([id]) => id);

  return {
    band: worst,
    decisions,
    // When everything is clean there is nothing blocking; an `auto` verdict
    // has no blockers by definition.
    blockers: worst === "auto" ? [] : blockers,
  };
}
