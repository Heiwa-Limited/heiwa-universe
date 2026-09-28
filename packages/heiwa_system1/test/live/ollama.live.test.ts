/**
 * Live verification of the structured-LLM fallback against a real model.
 *
 * Unlike `jev.live.test.ts`, this needs no account — it probes a local
 * Ollama and skips cleanly when none is running, so it is safe in CI.
 *
 *   OLLAMA_MODEL=qwen3.5:9b npm run test:live
 *
 * What makes this worth running: every other test of the fallback adapter
 * drives a stub that returns whatever we told it to. Only this one proves
 * the claims against a model that has its own opinions —
 *
 *   - that an uncalibrated model, given our generated JSON Schema, really
 *     is constrained to the offered options;
 *   - that the confidence cap actually holds end-to-end, so a fallback
 *     answer CANNOT reach the auto-dispatch band;
 *   - that the latency budget aborts a genuinely slow provider rather than
 *     hanging on it.
 */

import assert from "node:assert/strict";
import { describe, test } from "node:test";
import { probeModel } from "../../src/core/system1/adapters/probe.ts";
import {
  FALLBACK_CONFIDENCE_CEILING,
  memorySink,
  System1Client,
  structuredLlmAdapter,
} from "../../src/core/system1/index.ts";
import { INBOUND_QUESTIONS } from "../../src/examples/routing_agent/questions.ts";
import { DEFAULT_THRESHOLDS, gateBatch } from "../../src/orchestrator/index.ts";

const OLLAMA_URL = process.env.OLLAMA_URL ?? "http://127.0.0.1:11434";
/**
 * The model is DISCOVERED, not hardcoded.
 *
 * A default like "gemma4:latest" is a statement about the author's laptop.
 * A stranger with Ollama running but that model absent would get a
 * confusing failure instead of a skip. So the suite asks the endpoint what
 * it has, probes candidates, and runs against the first suitable one —
 * skipping with a clear reason if none qualifies.
 *
 * Set OLLAMA_MODEL to pin a specific model instead.
 */
let MODEL = process.env.OLLAMA_MODEL ?? "";
/** Local models are far slower than Jev; this is a correctness budget. */
const GENEROUS_BUDGET_MS = Number(process.env.OLLAMA_BUDGET_MS ?? 180_000);

const TICKET = {
  channel: "email",
  from: "customer@example.com",
  subject: "Duplicate charge on order A-104",
  message:
    "I was charged twice for order A-104. Please refund the duplicate charge.",
};

/**
 * Probed at module load, NOT in a `before()` hook.
 *
 * `describe`/`test` options are evaluated at registration time, which runs
 * before any hook. Deciding `skip` from a value a hook sets later means the
 * suite skips unconditionally — a test that always silently skips looks like
 * coverage while proving nothing. Top-level await runs first, so the flag is
 * correct by the time the suite registers.
 */
const reachable = await (async () => {
  try {
    const res = await fetch(`${OLLAMA_URL}/api/tags`, {
      signal: AbortSignal.timeout(2000),
    });
    if (!res.ok) return false;
    if (MODEL) return true;

    const body = (await res.json()) as { models?: Array<{ name?: unknown }> };
    const candidates = (body.models ?? [])
      .map((m) => (typeof m.name === "string" ? m.name : ""))
      .filter((n) => n.length > 0 && !/embed/i.test(n));

    // Probe rather than guess. A reasoning model looks fine in a model list
    // and then answers nothing; only a probe can tell them apart.
    for (const candidate of candidates) {
      const probe = await probeModel({
        model: candidate,
        baseUrl: OLLAMA_URL,
        timeoutMs: 120_000,
      });
      if (probe.suitable) {
        MODEL = candidate;
        return true;
      }
    }
    return false;
  } catch {
    return false;
  }
})();

if (!reachable) {
  console.log(
    `\n  (skipping Ollama suite: nothing reachable at ${OLLAMA_URL})`,
  );
}

function adapter() {
  return structuredLlmAdapter({ model: MODEL, baseUrl: OLLAMA_URL });
}

describe("live: structured-LLM fallback on real Ollama", () => {
  test("answers the whole fan and decodes cleanly", {
    skip: !reachable,
  }, async (t) => {
    if (!reachable) return t.skip();
    const telemetry = memorySink();
    const client = new System1Client({
      adapter: adapter(),
      budgetMs: GENEROUS_BUDGET_MS,
      telemetry,
    });

    const result = await client.evaluate({
      state: TICKET,
      questions: INBOUND_QUESTIONS,
    });
    if (!result.ok) {
      assert.fail(
        `live Ollama call failed: ${result.error.kind} — ${result.error.message}`,
      );
    }

    assert.deepEqual(
      Object.keys(result.value.answers).sort(),
      Object.keys(INBOUND_QUESTIONS).sort(),
      "a real model must still answer every question we asked",
    );
    assert.deepEqual(result.value.missing, []);

    console.log(
      `\n  ${MODEL}: ${result.value.latencyMs}ms · ${result.value.usage.inputTokens} in / ${result.value.usage.outputTokens} out`,
    );
    for (const q of telemetry.records[0]?.questions ?? []) {
      console.log(
        `    ${q.id.padEnd(12)} ${q.kind.padEnd(7)} conf=${q.confidence.toFixed(3)} (${q.confidenceSource})`,
      );
    }
  });

  test("is constrained to the options we offered", {
    skip: !reachable,
  }, async (t) => {
    if (!reachable) return t.skip();
    const client = new System1Client({
      adapter: adapter(),
      budgetMs: GENEROUS_BUDGET_MS,
    });
    const result = await client.evaluate({
      state: TICKET,
      questions: INBOUND_QUESTIONS,
    });
    if (!result.ok) assert.fail(`live call failed: ${result.error.message}`);

    const route = result.value.answers.route;
    assert.ok(route && route.kind === "choice");
    // The decoder would have rejected an unoffered key, so reaching here is
    // already the proof; assert it explicitly so the intent is legible.
    assert.ok(
      (INBOUND_QUESTIONS.route.options as readonly string[]).includes(
        route.choice,
      ),
      `model chose "${route.choice}", which was never offered`,
    );

    const urgency = result.value.answers.urgency;
    assert.ok(urgency && urgency.kind === "score");
    assert.ok(
      urgency.score >= 0 &&
        urgency.score <= INBOUND_QUESTIONS.urgency.levels - 1,
      `score ${urgency.score} outside the declared scale`,
    );
  });

  test("CANNOT reach the auto band — the confidence cap holds against a real model", {
    skip: !reachable,
  }, async (t) => {
    if (!reachable) return t.skip();
    const client = new System1Client({
      adapter: adapter(),
      budgetMs: GENEROUS_BUDGET_MS,
    });
    const result = await client.evaluate({
      state: TICKET,
      questions: INBOUND_QUESTIONS,
    });
    if (!result.ok) assert.fail(`live call failed: ${result.error.message}`);

    // This is the safety property that matters most: an uncalibrated model
    // must never be able to open the fast path on its own say-so.
    const gate = gateBatch(result.value.answers);
    for (const [id, answer] of Object.entries(result.value.answers)) {
      if (answer.kind === "noul") continue; // noul confidence is derived, not capped
      assert.ok(
        answer.confidence <= FALLBACK_CONFIDENCE_CEILING,
        `${id} reported ${answer.confidence}, above the ${FALLBACK_CONFIDENCE_CEILING} ceiling`,
      );
      assert.ok(
        answer.confidence <= DEFAULT_THRESHOLDS.auto,
        `${id} could reach the auto band from a degraded adapter`,
      );
      assert.notEqual(
        gate.decisions[id]?.band,
        "auto",
        `${id} landed in the auto band from an uncalibrated model`,
      );
    }
  });

  test("a 500ms System 1 budget aborts this provider rather than hanging on it", {
    skip: !reachable,
  }, async (t) => {
    if (!reachable) return t.skip();
    const client = new System1Client({ adapter: adapter(), budgetMs: 500 });

    const started = Date.now();
    const result = await client.evaluate({
      state: TICKET,
      questions: INBOUND_QUESTIONS,
    });
    const elapsed = Date.now() - started;

    // A local model will not answer a six-question fan in 500ms. The point
    // is that we find out in ~500ms, as a typed timeout, not by blocking.
    if (!result.ok) {
      assert.equal(result.error.kind, "timeout");
      assert.ok(
        elapsed < 3000,
        `budget overran: took ${elapsed}ms for a 500ms budget`,
      );
      console.log(
        `\n  500ms budget aborted after ${elapsed}ms (expected for a local model)`,
      );
    } else {
      console.log(
        `\n  ${MODEL} answered inside 500ms (${result.value.latencyMs}ms) — nice`,
      );
    }
  });
});
