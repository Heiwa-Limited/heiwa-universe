/**
 * One confidence scale for three different answer shapes.
 *
 * The gate in `orchestrator/gate.ts` compares every judgment against the same
 * thresholds, so every judgment must expose a comparable `confidence` in
 * [0, 1]. Jev gives that directly for `choice` and `score` — it is a summary
 * of how concentrated the probability distribution is. A `noul` answer has no
 * distribution to concentrate: it is a single probability, so Jev reports no
 * confidence at all.
 *
 * We derive one. A noul of 0.5 is maximal uncertainty; 0.0 and 1.0 are both
 * maximal certainty (being sure something is false is still being sure). The
 * honest mapping is distance from the coin flip, rescaled to [0, 1]:
 *
 *     confidence = |2p - 1|
 *
 * This is a Heiwa-side derivation, NOT a value TypeSafe returns. Anything that
 * reports confidence provenance must say so — see `Judgment.confidenceSource`.
 */

import type { DecodedAnswer } from "./types.ts";

/**
 * Derive a [0, 1] confidence from a noul probability.
 * Linear in distance from 0.5: `noulConfidence(0.75) === 0.5`.
 */
export function noulConfidence(p: number): number {
  return Math.abs(2 * p - 1);
}

/** Model-reported for choice/score; Heiwa-derived for noul. */
export type ConfidenceSource = "model" | "derived";

export function confidenceSourceOf(answer: DecodedAnswer): ConfidenceSource {
  return answer.kind === "noul" ? "derived" : "model";
}

/** The uniform [0, 1] confidence the gate reads, whatever the primitive. */
export function confidenceOf(answer: DecodedAnswer): number {
  switch (answer.kind) {
    case "choice":
      return answer.confidence;
    case "score":
      return answer.confidence;
    case "noul":
      return noulConfidence(answer.noul);
  }
}
