/**
 * Speculative evaluation: ask everything you might need, all at once.
 *
 * ## Why speculate at all
 *
 * The classic agent shape is sequential — classify, then branch, then check
 * security on the branch you took, then score urgency. Each hop pays a full
 * round trip, and you only learn the security answer after committing to a
 * route. Because Jev evaluates every question in a request independently and
 * in parallel against the same state, and prices output tokens at zero, the
 * cheap move is to ask *all* of them up front: routing target, security risk,
 * PII presence, sentiment, urgency. You pay one round trip and a handful of
 * extra input tokens, and the gate gets the whole picture before anything is
 * dispatched.
 *
 * "Speculative" in the CPU sense: compute branches you may not need, because
 * the parallel cost is lower than the serialised latency.
 *
 * ## Chunking
 *
 * `MAX_QUESTIONS_PER_REQUEST` caps one request. A larger fan is split into
 * chunks that are dispatched *concurrently*, so N chunks cost one chunk's
 * latency, not N. Chunk boundaries follow `Object.keys` order, which is
 * insertion order for string keys, so batching is deterministic and a failure
 * always names a stable question set.
 *
 * ## Partial results are first-class
 *
 * A failed chunk does not fail the fan. You get the answers that arrived plus
 * a `failures` list naming exactly which question ids are missing and why.
 * The gate can then decide: proceed on what is known, escalate, or quarantine.
 * Discarding 40 good answers because one chunk timed out would be the worse
 * trade in every case we care about.
 */

import {
  MAX_QUESTIONS_PER_REQUEST,
  type System1Client,
} from "../core/system1/client.ts";
import type { System1Error } from "../core/system1/errors.ts";
import type { Question } from "../core/system1/primitives.ts";
import type { TokenUsage } from "../core/system1/schema.ts";
import type { DecodedAnswer } from "../core/system1/types.ts";

export type ChunkFailure = {
  readonly questionIds: readonly string[];
  readonly error: System1Error;
};

export type SpeculativeResult = {
  readonly answers: Readonly<Record<string, DecodedAnswer>>;
  readonly failures: readonly ChunkFailure[];
  /** Questions asked that no chunk answered, whether by failure or omission. */
  readonly missing: readonly string[];
  /** True when every question asked came back with an answer. */
  readonly complete: boolean;
  /** Wall-clock time for the whole fan — the slowest chunk, not the sum. */
  readonly latencyMs: number;
  readonly usage: TokenUsage;
  readonly requests: number;
};

export type SpeculativeInput = {
  readonly state: unknown;
  readonly questions: Readonly<Record<string, Question>>;
};

export function chunkQuestions(
  questions: Readonly<Record<string, Question>>,
  size: number = MAX_QUESTIONS_PER_REQUEST,
): Array<Record<string, Question>> {
  const ids = Object.keys(questions);
  const chunks: Array<Record<string, Question>> = [];

  for (let i = 0; i < ids.length; i += size) {
    const chunk: Record<string, Question> = {};
    for (const id of ids.slice(i, i + size)) {
      const q = questions[id];
      if (q) chunk[id] = q;
    }
    chunks.push(chunk);
  }
  return chunks;
}

export async function speculate(
  client: System1Client,
  input: SpeculativeInput,
): Promise<SpeculativeResult> {
  const started = Date.now();
  const chunks = chunkQuestions(input.questions);

  if (chunks.length === 0) {
    return {
      // Null-prototype, exactly like the populated path: `answers` has one
      // shape on every code path so callers can index it without worrying
      // about inherited keys.
      answers: Object.create(null),
      failures: [],
      missing: [],
      complete: true,
      latencyMs: 0,
      usage: { inputTokens: 0, outputTokens: 0 },
      requests: 0,
    };
  }

  // `allSettled` over `all`: one rejected chunk must not cancel the others,
  // and the client does not reject anyway — this is belt and braces against
  // a future adapter that escapes the no-throw contract.
  const settled = await Promise.allSettled(
    chunks.map((questions) =>
      client.evaluate({ state: input.state, questions }),
    ),
  );

  const answers: Record<string, DecodedAnswer> = Object.create(null);
  const failures: ChunkFailure[] = [];
  let inputTokens = 0;
  let outputTokens = 0;

  settled.forEach((outcome, index) => {
    const questionIds = Object.keys(chunks[index] ?? {});

    if (outcome.status === "rejected") {
      failures.push({
        questionIds,
        error: {
          kind: "transport",
          message: `chunk ${index} rejected unexpectedly: ${String(outcome.reason)}`,
          retryable: true,
          cause: outcome.reason,
        },
      });
      return;
    }

    const result = outcome.value;
    if (!result.ok) {
      failures.push({ questionIds, error: result.error });
      return;
    }

    Object.assign(answers, result.value.answers);
    inputTokens += result.value.usage.inputTokens;
    outputTokens += result.value.usage.outputTokens;
  });

  const missing = Object.keys(input.questions).filter((id) => !(id in answers));

  return {
    answers,
    failures,
    missing,
    complete: missing.length === 0 && failures.length === 0,
    latencyMs: Date.now() - started,
    usage: { inputTokens, outputTokens },
    requests: chunks.length,
  };
}
