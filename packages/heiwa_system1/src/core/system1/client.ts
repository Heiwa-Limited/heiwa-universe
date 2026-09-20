/**
 * The System 1 client: one call, one latency budget, one typed outcome.
 *
 * Everything that is the same regardless of provider lives here — envelope
 * construction, the deadline, retry-with-backoff, decoding, telemetry — so an
 * adapter can stay dumb and a fallback cannot silently weaken the contract.
 *
 * ## The no-throw contract
 *
 * `evaluate` returns `Promise<Result<...>>` and never rejects. Adapter bugs,
 * socket failures, HTML error pages, provider drift, and blown deadlines all
 * arrive as typed `System1Error` values. This is what lets the orchestrator
 * treat "the provider returned nonsense" and "the model was unsure" as the
 * same class of event — both quarantine, neither crashes the pipeline.
 *
 * ## Why the question cap is a policy, not a limit
 *
 * Jev imposes no hard question count; the real ceiling is a shared ~64k token
 * budget across state + questions. `MAX_QUESTIONS_PER_REQUEST` is Heiwa's own
 * cap, chosen so a single speculative batch stays comfortably inside the
 * budget and inside the 500ms target. The orchestrator chunks larger fans out
 * across concurrent requests rather than raising it.
 */

import type { Adapter } from "./adapters/types.ts";
import { confidenceOf, confidenceSourceOf } from "./confidence.ts";
import {
  err,
  ok,
  type Result,
  type System1Error,
  system1Error,
} from "./errors.ts";
import {
  type CallTelemetry,
  noopSink,
  type QuestionTelemetry,
  type TelemetrySink,
} from "./observability.ts";
import type { Question, QuestionWire } from "./primitives.ts";
import { decodeResponse, type TokenUsage } from "./schema.ts";
import type { DecodedAnswer } from "./types.ts";

/** Heiwa's per-request question cap. See the note above — policy, not a limit. */
export const MAX_QUESTIONS_PER_REQUEST = 20;

/** The documented System 1 fast-path target. */
export const DEFAULT_BUDGET_MS = 500;

export const DEFAULT_MODEL = "jev-latest";

export type System1ClientOptions = {
  readonly adapter: Adapter;
  readonly model?: string;
  /** Latency budget for the whole call, retries included. */
  readonly budgetMs?: number;
  /** Attempts for retryable failures, including the first. */
  readonly maxAttempts?: number;
  readonly telemetry?: TelemetrySink;
  /** Injectable clock, so budget behaviour is testable without sleeping. */
  readonly now?: () => number;
};

export type EvaluateInput<Q extends Readonly<Record<string, Question>>> = {
  readonly state: unknown;
  readonly questions: Q;
};

export type EvaluateOutput = {
  readonly answers: Readonly<Record<string, DecodedAnswer>>;
  /** Questions this request asked that came back unanswered. */
  readonly missing: readonly string[];
  readonly model: string;
  readonly adapter: string;
  readonly latencyMs: number;
  readonly usage: TokenUsage;
  readonly attempts: number;
};

function backoffMs(attempt: number): number {
  // 25ms, 50ms, 100ms … deliberately small: this is a sub-second fast path,
  // not a background job. If backoff would exceed the budget we do not retry.
  return 25 * 2 ** (attempt - 1);
}

export class System1Client {
  readonly adapter: Adapter;
  readonly model: string;
  readonly budgetMs: number;
  readonly maxAttempts: number;
  readonly #telemetry: TelemetrySink;
  readonly #now: () => number;

  constructor(options: System1ClientOptions) {
    this.adapter = options.adapter;
    this.model = options.model ?? DEFAULT_MODEL;
    this.budgetMs = options.budgetMs ?? DEFAULT_BUDGET_MS;
    this.maxAttempts = options.maxAttempts ?? 1;
    this.#telemetry = options.telemetry ?? noopSink;
    this.#now = options.now ?? (() => Date.now());
  }

  async evaluate<Q extends Readonly<Record<string, Question>>>(
    input: EvaluateInput<Q>,
  ): Promise<Result<EvaluateOutput>> {
    const ids = Object.keys(input.questions);

    if (ids.length === 0) {
      return this.#fail(
        system1Error(
          "invalid_request",
          "evaluate() requires at least one question",
        ),
        0,
        0,
        0,
      );
    }
    if (ids.length > MAX_QUESTIONS_PER_REQUEST) {
      return this.#fail(
        system1Error(
          "invalid_request",
          `a single request carries at most ${MAX_QUESTIONS_PER_REQUEST} questions, received ${ids.length}; use the speculative dispatcher to chunk`,
        ),
        ids.length,
        0,
        0,
      );
    }

    const questionsWire: Record<string, QuestionWire> = {};
    for (const [id, q] of Object.entries(input.questions)) {
      questionsWire[id] = q.wire;
    }

    const body = {
      state: input.state,
      model: this.model,
      questions: questionsWire,
    };
    const started = this.#now();
    const deadline = started + this.budgetMs;

    let lastError: System1Error = system1Error(
      "transport",
      "no attempt was made",
    );
    let attempts = 0;

    for (let attempt = 1; attempt <= this.maxAttempts; attempt++) {
      attempts = attempt;
      const remaining = deadline - this.#now();
      if (remaining <= 0) {
        lastError = system1Error(
          "timeout",
          `latency budget of ${this.budgetMs}ms exhausted`,
          {
            adapter: this.adapter.name,
          },
        );
        break;
      }

      const attemptResult = await this.#attempt(body, remaining);

      if (attemptResult.ok) {
        const decoded = decodeResponse(input.questions, attemptResult.value);
        if (!decoded.ok) {
          // A schema violation is the provider's contract breaking, not a
          // transient blip. Retrying the same body would fail identically.
          return this.#fail(
            decoded.error,
            ids.length,
            this.#now() - started,
            attempt,
          );
        }

        const latencyMs = this.#now() - started;
        this.#emit({
          adapter: this.adapter.name,
          model: decoded.value.model,
          questionCount: ids.length,
          latencyMs,
          budgetMs: this.budgetMs,
          overBudget: latencyMs > this.budgetMs,
          attempts: attempt,
          inputTokens: decoded.value.usage.inputTokens,
          outputTokens: decoded.value.usage.outputTokens,
          questions: telemetryFor(decoded.value.answers, input.questions),
        });

        return ok({
          answers: decoded.value.answers,
          missing: decoded.value.missing,
          model: decoded.value.model,
          adapter: this.adapter.name,
          latencyMs,
          usage: decoded.value.usage,
          attempts: attempt,
        });
      }

      lastError = attemptResult.error;
      if (!lastError.retryable || attempt === this.maxAttempts) break;

      const wait = backoffMs(attempt);
      if (this.#now() + wait >= deadline) break;
      await sleep(wait);
    }

    return this.#fail(lastError, ids.length, this.#now() - started, attempts);
  }

  /** One adapter call under a deadline. Absorbs synchronous adapter throws. */
  async #attempt(
    body: Parameters<Adapter["evaluate"]>[0]["body"],
    remainingMs: number,
  ): Promise<Result<unknown>> {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), remainingMs);
    // `unref` keeps a pending budget timer from holding the process open.
    timer.unref?.();
    const timedOut = () =>
      system1Error(
        "timeout",
        `adapter "${this.adapter.name}" exceeded the latency budget`,
        {
          adapter: this.adapter.name,
        },
      );

    try {
      // Wrapped in Promise.resolve().then so an adapter that throws
      // synchronously is caught here rather than escaping to the caller.
      const result = await Promise.resolve().then(() =>
        this.adapter.evaluate({ body }, controller.signal),
      );
      if (controller.signal.aborted) return err(timedOut());
      return result;
    } catch (cause) {
      if (controller.signal.aborted) return err(timedOut());
      return err(
        system1Error(
          "transport",
          `adapter "${this.adapter.name}" threw: ${String(cause)}`,
          {
            adapter: this.adapter.name,
            cause,
          },
        ),
      );
    } finally {
      clearTimeout(timer);
    }
  }

  #fail(
    error: System1Error,
    questionCount: number,
    latencyMs: number,
    attempts: number,
  ): Result<EvaluateOutput> {
    this.#emit({
      adapter: this.adapter.name,
      model: this.model,
      questionCount,
      latencyMs,
      budgetMs: this.budgetMs,
      overBudget: latencyMs > this.budgetMs,
      attempts,
      inputTokens: 0,
      outputTokens: 0,
      questions: [],
      error,
    });
    return err(error);
  }

  #emit(record: CallTelemetry): void {
    // A broken telemetry sink must never take down a decision path.
    try {
      this.#telemetry(record);
    } catch {
      /* intentionally swallowed */
    }
  }
}

function telemetryFor(
  answers: Readonly<Record<string, DecodedAnswer>>,
  questions: Readonly<Record<string, Question>>,
): QuestionTelemetry[] {
  return Object.keys(questions).flatMap((id) => {
    const answer = answers[id];
    if (!answer) return [];
    return [
      {
        id,
        kind: answer.kind,
        confidence: confidenceOf(answer),
        confidenceSource: confidenceSourceOf(answer),
      },
    ];
  });
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => {
    const t = setTimeout(resolve, ms);
    t.unref?.();
  });
}
