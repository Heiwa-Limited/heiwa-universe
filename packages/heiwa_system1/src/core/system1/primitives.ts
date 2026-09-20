/**
 * The three System One primitives, as typed builders.
 *
 * A builder does two jobs at once:
 *
 *   1. It produces `wire` — the exact JSON object the Jev `/v1/systemone`
 *      endpoint expects under `questions[<id>]`. Nothing else in the codebase
 *      hand-writes that shape.
 *   2. It carries `kind` and phantom generics so the response decoder knows,
 *      at compile time, which answer shape comes back for this question and
 *      which option keys are legal — before a single byte crosses the network.
 *
 * Validation here is deliberately eager and throwing: a malformed *question*
 * is a programming error caught at construction, in the caller's own stack.
 * A malformed *answer* is a runtime/network condition and is never thrown —
 * see `client.ts`, which returns it as a typed failure instead.
 */

/** Jev's documented ceiling on the number of options in a single Choice. */
export const MAX_CHOICE_OPTIONS = 255;

export type PrimitiveKind = "choice" | "score" | "noul";

export type ChoiceWire<K extends string = string> = {
  readonly type: "choice";
  readonly instructions: string;
  readonly criteria: Readonly<Record<K, string>>;
};

export type ScoreWire = {
  readonly type: "score";
  readonly instructions: string;
  readonly criteria: readonly string[];
};

export type NoulWire = {
  readonly type: "noul";
  readonly instructions: string;
  readonly criteria?: { readonly yes?: string; readonly no?: string };
};

export type QuestionWire = ChoiceWire | ScoreWire | NoulWire;

export type ChoiceQuestion<K extends string = string> = {
  readonly kind: "choice";
  /** The legal option keys, preserved as a literal union for the decoder. */
  readonly options: readonly K[];
  readonly wire: ChoiceWire<K>;
};

export type ScoreQuestion = {
  readonly kind: "score";
  /** Number of levels on the scale; the score is a float in [0, levels - 1]. */
  readonly levels: number;
  readonly wire: ScoreWire;
};

export type NoulQuestion = {
  readonly kind: "noul";
  readonly wire: NoulWire;
};

export type Question<K extends string = string> =
  | ChoiceQuestion<K>
  | ScoreQuestion
  | NoulQuestion;

function requireInstructions(instructions: string): string {
  if (instructions.trim().length === 0) {
    throw new TypeError("instructions must not be empty");
  }
  return instructions;
}

export function choice<K extends string>(spec: {
  instructions: string;
  criteria: Record<K, string>;
}): ChoiceQuestion<K> {
  requireInstructions(spec.instructions);
  const options = Object.keys(spec.criteria) as K[];

  if (options.length < 2) {
    throw new TypeError(
      `a choice needs at least 2 options to be a decision, received ${options.length}`,
    );
  }
  if (options.length > MAX_CHOICE_OPTIONS) {
    throw new TypeError(
      `a choice accepts at most 255 options, received ${options.length}`,
    );
  }

  return {
    kind: "choice",
    options,
    wire: {
      type: "choice",
      instructions: spec.instructions,
      criteria: spec.criteria,
    },
  };
}

export function score(spec: {
  instructions: string;
  criteria: readonly string[];
}): ScoreQuestion {
  requireInstructions(spec.instructions);

  if (spec.criteria.length < 2) {
    throw new TypeError(
      `a score needs at least 2 levels to span a scale, received ${spec.criteria.length}`,
    );
  }

  return {
    kind: "score",
    levels: spec.criteria.length,
    wire: {
      type: "score",
      instructions: spec.instructions,
      criteria: spec.criteria,
    },
  };
}

export function noul(spec: {
  instructions: string;
  criteria?: { yes?: string; no?: string };
}): NoulQuestion {
  requireInstructions(spec.instructions);

  // `criteria` is omitted rather than sent as `undefined`: the wire object is
  // JSON-serialised verbatim and an explicit null/undefined key is noise.
  const wire: NoulWire = spec.criteria
    ? { type: "noul", instructions: spec.instructions, criteria: spec.criteria }
    : { type: "noul", instructions: spec.instructions };

  return { kind: "noul", wire };
}
