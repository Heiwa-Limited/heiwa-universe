/**
 * Domain types for decoded System 1 answers.
 *
 * Kept separate from `schema.ts` on purpose: `schema.ts` owns the zod wire
 * validators (what an untrusted HTTP body is allowed to look like), while this
 * file owns what the rest of the engine reasons about once a body has been
 * proven well-formed. Modules like `confidence.ts` and the orchestrator depend
 * only on this file, so they never pull zod into their import graph.
 */

/** A Choice answer: the selected key plus the full distribution over options. */
export type ChoiceAnswer<K extends string = string> = {
  readonly kind: "choice";
  readonly choice: K;
  readonly probabilities: Readonly<Record<K, number>>;
  readonly confidence: number;
};

/**
 * A Score answer. `score` is a probability-weighted float over the levels, so
 * a 3-level scale can legitimately return 1.5 — it is not an index.
 */
export type ScoreAnswer = {
  readonly kind: "score";
  readonly score: number;
  readonly legend: readonly string[];
  readonly probabilities: readonly number[];
  readonly confidence: number;
};

/**
 * A Noul answer: a single probability in [0, 1]. Jev reports no confidence for
 * this primitive; see `confidence.ts` for the derivation used by the gate.
 */
export type NoulAnswer = {
  readonly kind: "noul";
  readonly noul: number;
};

export type DecodedAnswer = ChoiceAnswer | ScoreAnswer | NoulAnswer;
