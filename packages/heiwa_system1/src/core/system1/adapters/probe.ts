/**
 * Preflight: is this model usable as a System 1 fallback?
 *
 * ## Why this exists
 *
 * Every finding below is one that cost real debugging time while building
 * this package, against a real local model. Writing them in a document does
 * not help the next person — they will point the adapter at whatever model
 * they have, watch a six-question fan fail after seventy-five seconds, and
 * have to work out why from first principles.
 *
 * So the findings are encoded as a test the product runs *for* them, in one
 * cheap request, with a remedy attached to each result. That is the whole
 * design intent: a lesson learned once should cost the next operator zero.
 *
 * ## What it checks, and why each one is here
 *
 * `reasons_before_answering` — BLOCKER. A reasoning model on an
 *   OpenAI-compatible route spends its output budget on chain-of-thought,
 *   returns an empty `content` with its thinking in a separate `reasoning`
 *   field, and does it slowly. Measured: qwen3.5 burned 3745 completion
 *   tokens and answered nothing. Neither `think: false` nor
 *   `chat_template_kwargs.enable_thinking` is honoured there.
 *
 * `ignores_enum` — BLOCKER. If the decoder is not really constrained, the
 *   central "cannot emit an unoffered option" property is gone and this
 *   model cannot be trusted for classification at all.
 *
 * `ignores_numeric_bounds` — WARNING. Measured: Ollama honours `enum` but
 *   ignores `minimum`/`maximum`, returning values like `noul: 4`. Not a
 *   blocker because `decodeResponse` rejects those downstream and the
 *   pipeline quarantines — but it costs throughput, so the operator should
 *   know before they see a quarantine rate they cannot explain.
 *
 * `emits_reasoning` — WARNING. The model answered, but spent tokens
 *   thinking first. It works; it is slower and closer to the budget than it
 *   looks.
 *
 * `unparseable`, `unreachable`, `timeout` — BLOCKER, self-explanatory.
 *
 * ## What it deliberately does NOT check
 *
 * Accuracy and calibration. One request cannot measure either, and a probe
 * that guessed at them would be worse than one that stays silent. Use
 * `orchestrator/calibration.ts` against a labelled corpus for that.
 */

import type { QuestionWire } from "../primitives.ts";
import { choice, noul, score } from "../primitives.ts";
import { questionsToJsonSchema } from "./structured_llm.ts";

export type ProbeCode =
  | "unreachable"
  | "timeout"
  | "unparseable"
  | "reasons_before_answering"
  | "emits_reasoning"
  | "ignores_enum"
  | "ignores_numeric_bounds"
  | "incomplete_answer";

export type ProbeFinding = {
  readonly code: ProbeCode;
  /** A blocker makes the model unusable here; a warning degrades it. */
  readonly severity: "blocker" | "warning";
  /** What was observed, in concrete terms, with the offending value. */
  readonly detail: string;
  /** What to do about it. Never empty. */
  readonly remedy: string;
};

export type ProbeResult = {
  readonly model: string;
  readonly reachable: boolean;
  /** True when nothing blocking was found. Warnings do not clear this flag. */
  readonly suitable: boolean;
  readonly latencyMs: number;
  readonly findings: readonly ProbeFinding[];
  /** One-paragraph rendering, safe to paste into an issue. */
  readonly summary: string;
};

export type ProbeOptions = {
  readonly model: string;
  readonly baseUrl: string;
  readonly apiKey?: string;
  readonly path?: string;
  /** Probe deadline. Generous by default: a slow answer is still a signal. */
  readonly timeoutMs?: number;
  readonly fetchImpl?: typeof fetch;
};

/**
 * Three questions, one of each primitive, with answers a competent model
 * cannot get wrong. We are testing the plumbing, not the intelligence — so
 * an incorrect answer here is not reported as a finding, only a malformed
 * or out-of-range one.
 */
const PROBE_QUESTIONS = {
  probe_choice: choice({
    instructions: "The state says the word 'yes'. Which option matches?",
    criteria: {
      true: "The state contains the word yes",
      false: "The state contains the word no",
    },
  }),
  probe_noul: noul({ instructions: "Does the state contain the word 'yes'?" }),
  probe_score: score({
    instructions: "How many words does the state contain?",
    criteria: ["zero words", "one word", "two or more words"],
  }),
};

const PROBE_STATE = "yes";

export async function probeModel(options: ProbeOptions): Promise<ProbeResult> {
  const findings: ProbeFinding[] = [];
  const timeoutMs = options.timeoutMs ?? 120_000;
  const doFetch = options.fetchImpl ?? fetch;
  const url = `${options.baseUrl.replace(/\/$/, "")}${options.path ?? "/v1/chat/completions"}`;

  const wire: Record<string, QuestionWire> = {};
  for (const [id, q] of Object.entries(PROBE_QUESTIONS)) wire[id] = q.wire;
  const schema = questionsToJsonSchema(wire);

  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  timer.unref?.();

  const started = Date.now();
  let response: Response;
  try {
    response = await doFetch(url, {
      method: "POST",
      signal: controller.signal,
      headers: {
        "content-type": "application/json",
        ...(options.apiKey
          ? { authorization: `Bearer ${options.apiKey}` }
          : {}),
      },
      body: JSON.stringify({
        model: options.model,
        temperature: 0,
        max_tokens: 1024,
        messages: [
          {
            role: "system",
            content:
              "You are a classification engine. You emit only JSON conforming to the supplied schema.",
          },
          {
            role: "user",
            content: `STATE:\n${PROBE_STATE}\n\nAnswer every question about the state.`,
          },
        ],
        response_format: {
          type: "json_schema",
          json_schema: { name: "probe", strict: true, schema },
        },
      }),
    });
  } catch (cause) {
    clearTimeout(timer);
    const latencyMs = Date.now() - started;
    const timedOut = controller.signal.aborted;
    return finish(options.model, false, latencyMs, [
      timedOut
        ? {
            code: "timeout",
            severity: "blocker",
            detail: `no response within ${timeoutMs}ms`,
            remedy:
              "Raise timeoutMs, use a smaller model, or check the endpoint is serving.",
          }
        : {
            code: "unreachable",
            severity: "blocker",
            detail: `could not reach ${url}: ${String(cause)}`,
            remedy: "Check the base URL and that the server is running.",
          },
    ]);
  } finally {
    clearTimeout(timer);
  }

  const latencyMs = Date.now() - started;

  if (!response.ok) {
    return finish(options.model, true, latencyMs, [
      {
        code: "unreachable",
        severity: "blocker",
        detail: `endpoint returned HTTP ${response.status}`,
        remedy:
          response.status === 404
            ? "Check the model name and the route path; the model may not be pulled."
            : "Check credentials and the endpoint configuration.",
      },
    ]);
  }

  let body: unknown;
  try {
    body = await response.json();
  } catch {
    return finish(options.model, true, latencyMs, [
      {
        code: "unparseable",
        severity: "blocker",
        detail: "endpoint returned a non-JSON body",
        remedy:
          "Confirm the URL points at an OpenAI-compatible chat completions route.",
      },
    ]);
  }

  const first = (
    body as {
      choices?: Array<{
        finish_reason?: unknown;
        message?: { content?: unknown; reasoning?: unknown };
      }>;
    }
  )?.choices?.[0];
  const content = first?.message?.content;
  const reasoning = first?.message?.reasoning;
  const reasoningChars = typeof reasoning === "string" ? reasoning.length : 0;
  const truncated = first?.finish_reason === "length";

  if (typeof content !== "string" || content.trim().length === 0) {
    findings.push(
      reasoningChars > 0
        ? {
            code: "reasons_before_answering",
            severity: "blocker",
            detail: `answered nothing after ${reasoningChars} chars of chain-of-thought${truncated ? " and hit the output budget" : ""}`,
            remedy:
              "Use a non-reasoning model for this path, or raise maxTokens far enough that it finishes thinking AND answers. On Ollama's OpenAI-compatible route, think:false and chat_template_kwargs.enable_thinking are NOT honoured.",
          }
        : {
            code: "unparseable",
            severity: "blocker",
            detail: "returned no assistant content",
            remedy:
              "Check the model is loaded and the route is a chat completions endpoint.",
          },
    );
    return finish(options.model, true, latencyMs, findings);
  }

  let answers: Record<string, Record<string, unknown>>;
  try {
    answers = JSON.parse(content) as Record<string, Record<string, unknown>>;
  } catch {
    return finish(options.model, true, latencyMs, [
      ...findings,
      {
        code: "unparseable",
        severity: "blocker",
        detail: `emitted content that is not JSON: ${content.slice(0, 80)}`,
        remedy:
          "This endpoint is not enforcing the response schema. Confirm it supports response_format json_schema, or use a model that does.",
      },
    ]);
  }

  if (reasoningChars > 0) {
    findings.push({
      code: "emits_reasoning",
      severity: "warning",
      detail: `answered, but emitted ${reasoningChars} chars of chain-of-thought first`,
      remedy:
        "Works, but is slower and nearer the output budget than it appears. Prefer a non-reasoning model if latency matters.",
    });
  }

  // --- enum conformity: the property the whole design leans on -------------
  const chosen = answers.probe_choice?.choice;
  if (chosen !== undefined && chosen !== "yes" && chosen !== "no") {
    findings.push({
      code: "ignores_enum",
      severity: "blocker",
      detail: `returned "${String(chosen)}", which was not one of the offered options`,
      remedy:
        "This decoder is not constrained by the schema, so an unoffered value can reach your gate. Use a backend that enforces response_format json_schema.",
    });
  }

  // --- numeric bounds: measured to be unenforced on Ollama -----------------
  const out: string[] = [];
  const noulValue = answers.probe_noul?.noul;
  if (typeof noulValue === "number" && (noulValue < 0 || noulValue > 1)) {
    out.push(`noul=${noulValue} (must be within [0, 1])`);
  }
  const scoreValue = answers.probe_score?.score;
  if (typeof scoreValue === "number" && (scoreValue < 0 || scoreValue > 2)) {
    out.push(`score=${scoreValue} (must be within [0, 2])`);
  }
  if (out.length > 0) {
    findings.push({
      code: "ignores_numeric_bounds",
      severity: "warning",
      detail: `returned out-of-range values: ${out.join(", ")}`,
      remedy:
        "The decoder honours enum but not minimum/maximum. Out-of-range answers are rejected downstream and quarantined, so this costs throughput rather than safety. Expect a higher quarantine rate.",
    });
  }

  for (const id of Object.keys(PROBE_QUESTIONS)) {
    if (!Object.hasOwn(answers, id)) {
      findings.push({
        code: "incomplete_answer",
        severity: "warning",
        detail: `omitted "${id}" from its response`,
        remedy:
          "The model skips questions. A missing gating answer quarantines rather than dispatching, so this costs throughput. Consider fewer questions per request.",
      });
    }
  }

  return finish(options.model, true, latencyMs, findings);
}

function finish(
  model: string,
  reachable: boolean,
  latencyMs: number,
  findings: readonly ProbeFinding[],
): ProbeResult {
  const blockers = findings.filter((f) => f.severity === "blocker");
  const suitable = reachable && blockers.length === 0;

  const lines = [
    `${model}: ${suitable ? "SUITABLE" : "NOT SUITABLE"} as a System 1 fallback (${latencyMs}ms for a 3-question probe)`,
  ];
  for (const f of findings) {
    lines.push(`  [${f.severity}] ${f.code}: ${f.detail}`);
    lines.push(`      → ${f.remedy}`);
  }
  if (findings.length === 0) {
    lines.push(
      "  no issues found: schema honoured, bounds respected, answered directly",
    );
  }

  return {
    model,
    reachable,
    suitable,
    latencyMs,
    findings,
    summary: lines.join("\n"),
  };
}
