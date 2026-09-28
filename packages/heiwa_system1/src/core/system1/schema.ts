/**
 * Wire validation: the single place an untrusted HTTP body becomes typed data.
 *
 * ## Why this exists when Jev "cannot" return an invalid value
 *
 * TypeSafe's guarantee is that the *model* is constrained to the schema, so it
 * cannot hallucinate an option you did not define. That guarantee covers the
 * model. It does not cover the bytes between the model and this process: a
 * proxy returning an HTML error page, a truncated body, an OpenRouter shim
 * that reshapes the envelope, a version skew after a provider deploy, or a
 * local fallback adapter backed by an ordinary LLM that has no such guarantee.
 *
 * So the engine's "0% parse failures" claim is made structurally, not by
 * trusting the provider: every body passes through `decodeResponse`, which
 * returns a `Result` and never throws. A hostile body becomes a
 * `schema_violation` that the gate quarantines, exactly like a low-confidence
 * judgment.
 *
 * ## Missing answers are reported, not fatal
 *
 * An earlier cut of this decoder rejected the whole body when any asked
 * question went unanswered. That is the wrong layer for the decision: it let
 * one missing *informational* answer destroy twenty good ones beside it. The
 * decoder's job is "is what came back trustworthy"; whether a gap matters is
 * policy, and policy lives at the gate — an unanswered PII check must
 * quarantine, an unanswered sentiment read must not. So `missing` is returned
 * alongside `answers`, and `orchestrator/pipeline.ts` decides.
 *
 * ## Contract checking beyond JSON shape
 *
 * zod proves the body is *shaped* right. It cannot know that `choice: "legal"`
 * is illegal unless it is told which options were asked. `decodeResponse`
 * therefore takes the question map and cross-checks each answer against the
 * question that produced it — option keys, distribution domain, scale length.
 * That is the part that catches provider drift.
 */

import { z } from "zod";

import { err, ok, type Result, system1Error } from "./errors.ts";
import type { Question } from "./primitives.ts";
import type { DecodedAnswer } from "./types.ts";

const probability = z.number().min(0).max(1);

/** `answers` values are validated per-question, so accept any record here. */
const rawAnswer = z.looseObject({});

const responseEnvelope = z.object({
  model: z.string().optional(),
  answers: z.record(z.string(), rawAnswer),
  usage: z
    .object({
      input_tokens: z.number().int().nonnegative().optional(),
      output_tokens: z.number().int().nonnegative().optional(),
    })
    .optional(),
});

// Every documented answer carries `type`. It is optional here so an adapter
// that emulates the contract may omit it, but when present it must name the
// question's own primitive — a cross-wired answer is a malformed answer.
const choiceAnswerShape = z.object({
  type: z.literal("choice").optional(),
  choice: z.string(),
  probabilities: z.record(z.string(), probability),
  confidence: probability,
});

// TypeSafe keys a Score's `legend` and `probabilities` by the level index as
// a string ("0", "1", …), per docs.typesafe.ai/api. `decodeOne` converts
// both maps into level order once the key set is proven to be the scale.
const scoreAnswerShape = z.object({
  type: z.literal("score").optional(),
  score: z.number(),
  legend: z.record(z.string(), z.string()),
  probabilities: z.record(z.string(), probability),
  confidence: probability,
});

const noulAnswerShape = z.object({
  type: z.literal("noul").optional(),
  noul: probability,
});

/** `"0".."levels-1"` — the only key set a Score map may carry. */
function levelKeys(levels: number): string[] {
  return Array.from({ length: levels }, (_, level) => String(level));
}

/**
 * Key level-ordered values by level index, the shape TypeSafe puts on the
 * wire for a Score's `legend` and `probabilities`. Adapters that emulate the
 * contract use this so every provider feeds the one decoder above. Values
 * pass through unrepaired — a malformed value is the decoder's to reject.
 */
export function byLevel<T>(values: readonly T[]): Record<string, T> {
  return Object.fromEntries(
    values.map((value, level) => [String(level), value]),
  );
}

export type TokenUsage = {
  readonly inputTokens: number;
  readonly outputTokens: number;
};

export type DecodedResponse = {
  readonly model: string;
  readonly answers: Readonly<Record<string, DecodedAnswer>>;
  /** Questions we asked that the provider returned no answer for. */
  readonly missing: readonly string[];
  readonly usage: TokenUsage;
};

function sameKeySet(a: readonly string[], b: readonly string[]): boolean {
  if (a.length !== b.length) return false;
  const set = new Set(a);
  return b.every((k) => set.has(k));
}

/**
 * Validate a provider body against the questions that produced it.
 *
 * Never throws. Answers to questions we did not ask are dropped rather than
 * rejected — a provider adding a field is not a reason to quarantine a
 * decision. Questions we did ask but got no answer for are listed in
 * `missing`; an answer that came back *malformed* is still a hard failure,
 * because a wrong answer is more dangerous than an absent one.
 */
export function decodeResponse(
  questions: Readonly<Record<string, Question>>,
  body: unknown,
): Result<DecodedResponse> {
  const envelope = responseEnvelope.safeParse(body);
  if (!envelope.success) {
    return err(
      system1Error(
        "schema_violation",
        `response envelope did not validate: ${z.prettifyError(envelope.error)}`,
        { cause: envelope.error },
      ),
    );
  }

  const answers: Record<string, DecodedAnswer> = Object.create(null);
  const missing: string[] = [];

  for (const [id, question] of Object.entries(questions)) {
    // `Object.hasOwn` rather than `in`: an inherited key from a polluted
    // prototype must not count as an answer.
    if (!Object.hasOwn(envelope.data.answers, id)) {
      missing.push(id);
      continue;
    }
    const raw = envelope.data.answers[id];

    const decoded = decodeOne(id, question, raw);
    if (!decoded.ok) return decoded;
    answers[id] = decoded.value;
  }

  return ok({
    model: envelope.data.model ?? "unknown",
    answers,
    missing,
    usage: {
      inputTokens: envelope.data.usage?.input_tokens ?? 0,
      outputTokens: envelope.data.usage?.output_tokens ?? 0,
    },
  });
}

function decodeOne(
  id: string,
  question: Question,
  raw: unknown,
): Result<DecodedAnswer> {
  switch (question.kind) {
    case "choice": {
      const parsed = choiceAnswerShape.safeParse(raw);
      if (!parsed.success) {
        return err(
          system1Error(
            "schema_violation",
            `choice answer "${id}" did not validate: ${z.prettifyError(parsed.error)}`,
            { cause: parsed.error },
          ),
        );
      }
      const legal = question.options as readonly string[];
      if (!legal.includes(parsed.data.choice)) {
        return err(
          system1Error(
            "schema_violation",
            `choice answer "${id}" selected "${parsed.data.choice}", which was not offered (${legal.join(", ")})`,
          ),
        );
      }
      const returned = Object.keys(parsed.data.probabilities);
      if (!sameKeySet(legal, returned)) {
        return err(
          system1Error(
            "schema_violation",
            `choice answer "${id}" returned a distribution over [${returned.join(", ")}] but the question offered [${legal.join(", ")}]`,
          ),
        );
      }
      return ok({
        kind: "choice",
        choice: parsed.data.choice,
        probabilities: parsed.data.probabilities,
        confidence: parsed.data.confidence,
      });
    }

    case "score": {
      const parsed = scoreAnswerShape.safeParse(raw);
      if (!parsed.success) {
        return err(
          system1Error(
            "schema_violation",
            `score answer "${id}" did not validate: ${z.prettifyError(parsed.error)}`,
            { cause: parsed.error },
          ),
        );
      }
      const levels = levelKeys(question.levels);
      const returned = Object.keys(parsed.data.probabilities);
      if (!sameKeySet(levels, returned)) {
        return err(
          system1Error(
            "schema_violation",
            `score answer "${id}" returned probabilities for levels [${returned.join(", ")}] on a ${question.levels}-level scale`,
          ),
        );
      }
      const legendKeys = Object.keys(parsed.data.legend);
      if (!sameKeySet(levels, legendKeys)) {
        return err(
          system1Error(
            "schema_violation",
            `score answer "${id}" returned a legend for levels [${legendKeys.join(", ")}] on a ${question.levels}-level scale`,
          ),
        );
      }
      if (parsed.data.score < 0 || parsed.data.score > question.levels - 1) {
        return err(
          system1Error(
            "schema_violation",
            `score answer "${id}" returned ${parsed.data.score}, outside the [0, ${question.levels - 1}] scale`,
          ),
        );
      }
      // Key sets are proven equal to the scale above, so every lookup hits.
      const { legend, probabilities } = parsed.data;
      return ok({
        kind: "score",
        score: parsed.data.score,
        legend: levels.map((level) => legend[level] ?? ""),
        probabilities: levels.map((level) => probabilities[level] ?? 0),
        confidence: parsed.data.confidence,
      });
    }

    case "noul": {
      const parsed = noulAnswerShape.safeParse(raw);
      if (!parsed.success) {
        return err(
          system1Error(
            "schema_violation",
            `noul answer "${id}" did not validate: ${z.prettifyError(parsed.error)}`,
            { cause: parsed.error },
          ),
        );
      }
      return ok({ kind: "noul", noul: parsed.data.noul });
    }
  }
}
