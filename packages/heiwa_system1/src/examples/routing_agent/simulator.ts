/**
 * An offline stand-in for Jev.
 *
 * Two jobs. It lets the demo run with no API key and no network, and it gives
 * the test suite a provider whose answers are deterministic, so gate
 * behaviour can be asserted exactly rather than probabilistically.
 *
 * It is NOT a model. Answers come from crude keyword heuristics over the
 * state. Nothing here should ever be read as evidence about Jev's accuracy —
 * it only produces well-formed answers in the right wire shape. Any test that
 * cares about a specific answer passes it in via `overrides` rather than
 * relying on a heuristic guessing right.
 */

import type { QuestionWire } from "../../core/system1/primitives.ts";
import { byLevel } from "../../core/system1/schema.ts";

function textOf(state: unknown): string {
  return (
    typeof state === "string" ? state : JSON.stringify(state ?? "")
  ).toLowerCase();
}

function hits(text: string, words: readonly string[]): number {
  return words.reduce((n, w) => (text.includes(w) ? n + 1 : n), 0);
}

/** Spread a peaked distribution over `keys`, favouring `winner`. */
function peaked(
  keys: readonly string[],
  winner: string,
  peak: number,
): Record<string, number> {
  const rest = keys.length > 1 ? (1 - peak) / (keys.length - 1) : 0;
  return Object.fromEntries(keys.map((k) => [k, k === winner ? peak : rest]));
}

function peakedArray(n: number, index: number, peak: number): number[] {
  const rest = n > 1 ? (1 - peak) / (n - 1) : 0;
  return Array.from({ length: n }, (_, i) => (i === index ? peak : rest));
}

const SIGNALS = {
  billing: [
    "refund",
    "charge",
    "charged",
    "invoice",
    "payment",
    "billed",
    "subscription",
  ],
  technical: [
    "error",
    "bug",
    "500",
    "outage",
    "broken",
    "api",
    "crash",
    "timeout",
  ],
  account: ["password", "login", "access", "cancel", "delete my", "export"],
  spam: ["unsubscribe", "webinar", "limited offer", "crypto"],
  injection: [
    "ignore all previous",
    "ignore previous",
    "system prompt",
    "you are now",
    "disregard your",
  ],
  pii: ["ssn", "passport", "card number", "iban", "sort code", "date of birth"],
  angry: ["unacceptable", "furious", "lawyer", "terrible", "worst", "scam"],
  urgent: ["urgent", "immediately", "asap", "right now", "outage", "fraud"],
  generation: [
    "explain",
    "explanation",
    "write",
    "draft",
    "apology",
    "why did",
    "detailed",
  ],
} as const;

/**
 * Produce a Jev-shaped response body for the given questions and state.
 * `overrides` replaces a whole answer by question id.
 */
export function simulateJev(
  questions: Readonly<Record<string, unknown>>,
  state: unknown,
  overrides: Readonly<Record<string, unknown>> = {},
): {
  model: string;
  answers: Record<string, unknown>;
  usage: { input_tokens: number; output_tokens: number };
} {
  const text = textOf(state);
  const answers: Record<string, unknown> = {};

  for (const [id, rawQuestion] of Object.entries(questions)) {
    if (Object.hasOwn(overrides, id)) {
      answers[id] = overrides[id];
      continue;
    }
    const q = rawQuestion as QuestionWire;

    if (q.type === "choice") {
      const keys = Object.keys(q.criteria);
      const scored = keys.map((k) => ({
        key: k,
        n: hits(text, SIGNALS[k as keyof typeof SIGNALS] ?? []),
      }));
      const best = scored.reduce(
        (a, b) => (b.n > a.n ? b : a),
        scored[0] ?? { key: keys[0] ?? "", n: 0 },
      );
      // A decisive keyword match reads as confident; no match reads as unsure,
      // which is exactly the case the gate should quarantine.
      const peak = best.n >= 2 ? 0.94 : best.n === 1 ? 0.88 : 0.4;
      answers[id] = {
        choice: best.key,
        probabilities: peaked(keys, best.key, peak),
        confidence: best.n >= 2 ? 0.95 : best.n === 1 ? 0.9 : 0.35,
      };
      continue;
    }

    if (q.type === "score") {
      const levels = q.criteria.length;
      const words = id === "urgency" ? SIGNALS.urgent : SIGNALS.angry;
      const index = Math.min(hits(text, words), levels - 1);
      answers[id] = {
        score: index,
        // TypeSafe keys a Score's legend and distribution by level index.
        legend: byLevel(q.criteria),
        probabilities: byLevel(peakedArray(levels, index, 0.9)),
        confidence: 0.9,
      };
      continue;
    }

    // noul
    let p = 0.05;
    if (id === "injection") p = hits(text, SIGNALS.injection) > 0 ? 0.96 : 0.02;
    else if (id === "pii") p = hits(text, SIGNALS.pii) > 0 ? 0.95 : 0.03;
    else if (id === "automatable") {
      p = hits(text, SIGNALS.generation) > 0 ? 0.05 : 0.95;
    }
    answers[id] = { noul: p };
  }

  return {
    model: "jev-simulated",
    answers,
    usage: {
      input_tokens: Math.max(1, Math.ceil(text.length / 4)),
      output_tokens: Object.keys(questions).length * 3,
    },
  };
}
