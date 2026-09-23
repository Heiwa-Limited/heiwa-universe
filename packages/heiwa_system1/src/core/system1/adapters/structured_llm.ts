/**
 * Fallback adapter: emulate System One on an ordinary structured-output LLM.
 *
 * Speaks the OpenAI-compatible `/v1/chat/completions` shape, so it covers
 * Ollama, vLLM, llama.cpp, OpenRouter's chat models, and the commercial APIs
 * without a per-provider branch. In Heiwa terms this is what keeps the fast
 * path available when the network is gone: a local qwen answering through
 * Ollama is a worse System 1 than Jev, but it is a System 1.
 *
 * ## Structure is enforced, calibration cannot be
 *
 * Two of Jev's guarantees survive the translation and one does not.
 *
 * PARTLY SURVIVES — schema conformity. `questionsToJsonSchema` emits `enum`
 * over exactly the offered option keys, `required` on every question, and
 * `additionalProperties: false`, and those ARE enforced: a constrained
 * decoder cannot invent an option we did not offer.
 *
 * Numeric bounds are a different story. Measured against a live Ollama, its
 * schema constraint honours structure and `enum` but IGNORES `minimum` and
 * `maximum` — one real run returned `noul: 2`, `noul: 4` and `score: -0.8`.
 * So the guarantee here is weaker than Jev's and must not be described as
 * equivalent. Out-of-range values are passed through unrepaired for
 * `decodeResponse` to reject; clamping them would turn "the model returned
 * nonsense" into "we confidently made an answer up", which is worse.
 *
 * DOES NOT SURVIVE — calibration. Jev's probabilities are trained against
 * outcomes. An LLM asked "how confident are you?" produces a number that
 * correlates with fluency, not with being right, and it is famously
 * overconfident. Taking it at face value would let a degraded path open the
 * auto-dispatch gate, which is the single most dangerous thing this package
 * could do.
 *
 * So confidence from this adapter is **capped**, not trusted:
 *
 *   reported_confidence = min(model_confidence, FALLBACK_CONFIDENCE_CEILING)
 *
 * The ceiling sits at the gate's `auto` threshold, so by construction a
 * fallback answer lands in `deliberate` at best — System 2 reviews it, or a
 * human does. A low self-reported confidence is passed through unchanged
 * (the cap only ever lowers), because a model admitting uncertainty is the
 * one signal here worth believing. An operator who has calibrated their own
 * model can raise `confidenceCeiling`, but has to say so out loud.
 */

import {
  err,
  errorKindForStatus,
  ok,
  type Result,
  system1Error,
} from "../errors.ts";
import type { QuestionWire } from "../primitives.ts";
import { byLevel } from "../schema.ts";
import type { Adapter, AdapterRequest } from "./types.ts";

/**
 * The most confidence an uncalibrated model is allowed to claim. Equal to the
 * gate's default `auto` threshold, which is a strict `>` comparison — so a
 * capped answer cannot reach the fast path. Asserted in the test suite.
 */
export const FALLBACK_CONFIDENCE_CEILING = 0.85;

/**
 * Base output allowance, plus {@link TOKENS_PER_QUESTION} for each question.
 *
 * Sized generously on purpose. The token cap is a BACKSTOP against a
 * pathological model, not the runaway control — the client's
 * `AbortController` latency budget is, and it fires in 500ms regardless of
 * how many tokens were allowed. A first cut used 256 + 96/question, which
 * measured out at 832 for a six-question fan and cut gemma4 off mid-answer
 * on 2 of 16 real tickets (it emitted ~870 tokens of chain-of-thought on the
 * harder ones). A tight cap manufactures failures; the wall clock is what
 * actually needs bounding.
 */
export const BASE_OUTPUT_TOKENS = 512;
export const TOKENS_PER_QUESTION = 256;

/** The default output budget for a fan of `n` questions. */
export function defaultMaxTokens(questionCount: number): number {
  return BASE_OUTPUT_TOKENS + TOKENS_PER_QUESTION * questionCount;
}

export type StructuredLlmOptions = {
  readonly model: string;
  readonly baseUrl: string;
  readonly apiKey?: string;
  readonly path?: string;
  /** Raise only with a calibrated model and eyes open. */
  readonly confidenceCeiling?: number;
  /**
   * Output token budget. Always sent — see the reasoning-model note above.
   * Defaults to a per-question allowance, which is ample for typed answers
   * and small enough that a runaway model fails fast instead of hanging.
   */
  readonly maxTokens?: number;
  readonly temperature?: number;
  readonly fetchImpl?: typeof fetch;
};

type JsonSchema = {
  type: "object";
  properties: Record<string, unknown>;
  required: string[];
  additionalProperties: false;
};

/**
 * Translate the question map into a JSON Schema the model is decoded against.
 * This is where "0% invalid values" is reconstructed for a model that has no
 * native constraint: the grammar simply cannot produce an unoffered key.
 */
export function questionsToJsonSchema(
  questions: Readonly<Record<string, QuestionWire>>,
): JsonSchema {
  const properties: Record<string, unknown> = {};
  const required: string[] = [];

  for (const [id, q] of Object.entries(questions)) {
    required.push(id);

    if (q.type === "choice") {
      const keys = Object.keys(q.criteria);
      properties[id] = {
        type: "object",
        description: q.instructions,
        properties: {
          choice: { type: "string", enum: keys },
          probabilities: {
            type: "object",
            properties: Object.fromEntries(
              keys.map((k) => [k, { type: "number", minimum: 0, maximum: 1 }]),
            ),
            additionalProperties: false,
          },
          confidence: { type: "number", minimum: 0, maximum: 1 },
        },
        required: ["choice"],
        additionalProperties: false,
      };
    } else if (q.type === "score") {
      properties[id] = {
        type: "object",
        description: q.instructions,
        properties: {
          score: { type: "number", minimum: 0, maximum: q.criteria.length - 1 },
          probabilities: {
            type: "array",
            items: { type: "number", minimum: 0, maximum: 1 },
            minItems: q.criteria.length,
            maxItems: q.criteria.length,
          },
          confidence: { type: "number", minimum: 0, maximum: 1 },
        },
        required: ["score"],
        additionalProperties: false,
      };
    } else {
      properties[id] = {
        type: "object",
        description: q.instructions,
        properties: { noul: { type: "number", minimum: 0, maximum: 1 } },
        required: ["noul"],
        additionalProperties: false,
      };
    }
  }

  return { type: "object", properties, required, additionalProperties: false };
}

function renderPrompt(
  state: unknown,
  questions: Readonly<Record<string, QuestionWire>>,
): string {
  const lines: string[] = [
    "Evaluate the STATE below against each question independently.",
    "Answer only from the state. Do not explain. Emit JSON matching the schema.",
    "",
    "STATE:",
    typeof state === "string" ? state : JSON.stringify(state, null, 2),
    "",
    "QUESTIONS:",
  ];

  for (const [id, q] of Object.entries(questions)) {
    if (q.type === "choice") {
      lines.push(`- ${id} (choice): ${q.instructions}`);
      for (const [key, desc] of Object.entries(q.criteria))
        lines.push(`    ${key}: ${desc}`);
    } else if (q.type === "score") {
      lines.push(
        `- ${id} (score 0..${q.criteria.length - 1}): ${q.instructions}`,
      );
      q.criteria.forEach((level, i) => {
        lines.push(`    ${i}: ${level}`);
      });
    } else {
      lines.push(
        `- ${id} (noul, probability 0..1 that this is true): ${q.instructions}`,
      );
      if (q.criteria?.true) lines.push(`    true means: ${q.criteria.true}`);
      if (q.criteria?.false) lines.push(`    false means: ${q.criteria.false}`);
    }
  }

  return lines.join("\n");
}

function uniform(n: number): number[] {
  return Array.from({ length: n }, () => 1 / n);
}

export function structuredLlmAdapter(options: StructuredLlmOptions): Adapter {
  const name = `structured-llm(${options.model})`;
  const ceiling = options.confidenceCeiling ?? FALLBACK_CONFIDENCE_CEILING;
  const doFetch = options.fetchImpl ?? fetch;
  const url = `${options.baseUrl.replace(/\/$/, "")}${options.path ?? "/v1/chat/completions"}`;

  return {
    name,
    async evaluate(
      req: AdapterRequest,
      signal: AbortSignal,
    ): Promise<Result<unknown>> {
      const questions = req.body.questions;
      const schema = questionsToJsonSchema(questions);

      let response: Response;
      try {
        response = await doFetch(url, {
          method: "POST",
          signal,
          headers: {
            "content-type": "application/json",
            ...(options.apiKey
              ? { authorization: `Bearer ${options.apiKey}` }
              : {}),
          },
          body: JSON.stringify({
            model: options.model,
            temperature: options.temperature ?? 0,
            max_tokens:
              options.maxTokens ??
              defaultMaxTokens(Object.keys(questions).length),
            messages: [
              {
                role: "system",
                content:
                  "You are a classification engine. You emit only JSON conforming to the supplied schema.",
              },
              {
                role: "user",
                content: renderPrompt(req.body.state, questions),
              },
            ],
            response_format: {
              type: "json_schema",
              json_schema: { name: "system_one_answers", strict: true, schema },
            },
          }),
        });
      } catch (cause) {
        if (signal.aborted) {
          return err(
            system1Error("timeout", `${name} aborted on the latency budget`, {
              adapter: name,
              cause,
            }),
          );
        }
        return err(
          system1Error(
            "transport",
            `${name} could not reach ${url}: ${String(cause)}`,
            { adapter: name, cause },
          ),
        );
      }

      if (!response.ok) {
        return err(
          system1Error(
            errorKindForStatus(response.status),
            `${name} returned HTTP ${response.status}`,
            {
              adapter: name,
              status: response.status,
            },
          ),
        );
      }

      let completion: unknown;
      try {
        completion = await response.json();
      } catch (cause) {
        return err(
          system1Error("schema_violation", `${name} returned a non-JSON body`, {
            adapter: name,
            cause,
          }),
        );
      }

      const choice = (
        completion as {
          choices?: Array<{
            finish_reason?: unknown;
            message?: { content?: unknown; reasoning?: unknown };
          }>;
        }
      )?.choices?.[0];
      const content = choice?.message?.content;
      const truncated = choice?.finish_reason === "length";
      const reasoning = choice?.message?.reasoning;
      const reasoningChars =
        typeof reasoning === "string" ? reasoning.length : 0;

      // Diagnose BEFORE parsing. An empty body from a truncated response is
      // not a syntax error, and calling it one sends the operator to the
      // wrong fix — raise the budget or change model, not repair the schema.
      if (typeof content !== "string" || content.trim().length === 0) {
        if (truncated) {
          return err(
            system1Error(
              "schema_violation",
              reasoningChars > 0
                ? `${name} hit its output budget while still emitting reasoning (${reasoningChars} chars of chain-of-thought, no answer). This model reasons before answering; raise maxTokens or use a non-reasoning model.`
                : `${name} was truncated at its output budget before emitting an answer; raise maxTokens.`,
              { adapter: name },
            ),
          );
        }
        return err(
          system1Error(
            "schema_violation",
            reasoningChars > 0
              ? `${name} returned only reasoning (${reasoningChars} chars) and an empty answer.`
              : `${name} returned no assistant content to decode`,
            { adapter: name },
          ),
        );
      }

      let raw: unknown;
      try {
        raw = JSON.parse(content);
      } catch (cause) {
        return err(
          system1Error(
            "schema_violation",
            `${name} emitted content that is not JSON`,
            { adapter: name, cause },
          ),
        );
      }

      const usage = (
        completion as {
          usage?: { prompt_tokens?: number; completion_tokens?: number };
        }
      ).usage;

      return ok({
        model: options.model,
        answers: translate(raw, questions, ceiling),
        usage: {
          input_tokens: usage?.prompt_tokens ?? 0,
          output_tokens: usage?.completion_tokens ?? 0,
        },
      });
    },
  };
}

/**
 * Reshape the model's JSON into System One answers.
 *
 * Deliberately permissive: anything malformed is passed through as-is so the
 * real decoder rejects it with a precise message, rather than being patched
 * up here into something that merely looks valid.
 */
function translate(
  raw: unknown,
  questions: Readonly<Record<string, QuestionWire>>,
  ceiling: number,
): Record<string, unknown> {
  if (typeof raw !== "object" || raw === null) return {};
  const source = raw as Record<string, unknown>;
  const answers: Record<string, unknown> = {};

  for (const [id, q] of Object.entries(questions)) {
    const value = Object.hasOwn(source, id) ? source[id] : undefined;
    if (typeof value !== "object" || value === null) continue;
    const a = value as Record<string, unknown>;

    // The cap only ever lowers. A model admitting low confidence is the one
    // number here worth believing, so it is preserved.
    const capped = Math.min(
      typeof a.confidence === "number" ? a.confidence : ceiling,
      ceiling,
    );

    if (q.type === "choice") {
      const keys = Object.keys(q.criteria);
      const probabilities =
        typeof a.probabilities === "object" && a.probabilities !== null
          ? (a.probabilities as Record<string, number>)
          : Object.fromEntries(
              keys.map((k) => [
                k,
                a.choice === k
                  ? capped
                  : (1 - capped) / Math.max(keys.length - 1, 1),
              ]),
            );
      answers[id] = { choice: a.choice, probabilities, confidence: capped };
    } else if (q.type === "score") {
      // The model answers in the array shape our JSON Schema asks for; the
      // wire contract keys levels by index, so re-key rather than reshape.
      answers[id] = {
        score: a.score,
        legend: byLevel(q.criteria),
        probabilities: Array.isArray(a.probabilities)
          ? byLevel(a.probabilities as unknown[])
          : byLevel(uniform(q.criteria.length)),
        confidence: capped,
      };
    } else {
      // Noul carries no confidence field on the wire; the gate derives it
      // from the probability, and that derivation is honest for any source.
      answers[id] = { noul: a.noul };
    }
  }

  return answers;
}
