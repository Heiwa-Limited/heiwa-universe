/**
 * "Which of my models can actually do this?"
 *
 *   node src/examples/routing_agent/doctor.ts
 *
 * Discovers the local models you already have and probes each one, then
 * tells you which are usable as a System 1 fallback and why the others are
 * not. Nothing is configured, nothing is assumed about your setup, and it
 * degrades to a clear message when there is nothing to talk to.
 *
 * This exists because the alternative is a document. The findings it reports
 * were each discovered the slow way — pointing the adapter at a model and
 * watching a six-question fan fail after seventy-five seconds. Encoding them
 * costs the next person nothing.
 *
 *   OLLAMA_URL      override the endpoint (default http://127.0.0.1:11434)
 *   HEIWA_S1_MODELS comma-separated list, to probe specific models instead
 */

import {
  type ProbeResult,
  probeModel,
} from "../../core/system1/adapters/probe.ts";

const OLLAMA_URL = process.env.OLLAMA_URL ?? "http://127.0.0.1:11434";

/** Ask the endpoint what it has. Returns [] if it cannot be reached. */
async function discoverModels(): Promise<string[]> {
  try {
    const res = await fetch(`${OLLAMA_URL}/api/tags`, {
      signal: AbortSignal.timeout(3000),
    });
    if (!res.ok) return [];
    const body = (await res.json()) as { models?: Array<{ name?: unknown }> };
    return (
      (body.models ?? [])
        .map((m) => (typeof m.name === "string" ? m.name : ""))
        .filter((n) => n.length > 0)
        // Embedding models cannot answer a chat completion; probing them would
        // report a confusing blocker for a model that was never a candidate.
        .filter((n) => !/embed/i.test(n))
    );
  } catch {
    return [];
  }
}

const requested = process.env.HEIWA_S1_MODELS?.split(",")
  .map((s) => s.trim())
  .filter(Boolean);
const models = requested ?? (await discoverModels());

console.log(`\nSystem 1 fallback doctor — endpoint ${OLLAMA_URL}\n`);

if (models.length === 0) {
  console.log(
    requested
      ? "  No models given in HEIWA_S1_MODELS."
      : `  No local models found at ${OLLAMA_URL}.\n\n` +
          `  This is not an error — the fallback adapter is optional. Heiwa's\n` +
          `  primary System 1 path is TypeSafe Jev, which needs no local model.\n` +
          `  To use a local fallback, install Ollama and pull a NON-reasoning\n` +
          `  instruct model, then run this again.`,
  );
  process.exit(0);
}

console.log(
  `  Probing ${models.length} model(s). Each probe is one small request.\n`,
);

const results: ProbeResult[] = [];
for (const model of models) {
  process.stdout.write(`  ${model} … `);
  const result = await probeModel({
    model,
    baseUrl: OLLAMA_URL,
    timeoutMs: 120_000,
  });
  results.push(result);
  console.log(
    `${result.suitable ? "OK" : "unsuitable"} (${(result.latencyMs / 1000).toFixed(1)}s)`,
  );
}

console.log(`\n${"─".repeat(68)}`);
for (const r of results) {
  console.log(`\n${r.summary}`);
}

const usable = results.filter((r) => r.suitable);
console.log(`\n${"─".repeat(68)}`);

if (usable.length === 0) {
  console.log(
    `\n  No local model here is usable as a System 1 fallback.\n\n` +
      `  The usual cause is a reasoning model: on an OpenAI-compatible route it\n` +
      `  spends its output budget on chain-of-thought and answers nothing, and\n` +
      `  thinking cannot be switched off there. Pull a plain instruct model.\n\n` +
      `  This does not block Heiwa. The fallback is a degraded path for when\n` +
      `  the network is gone; the primary path is unaffected.\n`,
  );
  process.exit(0);
}

// Fastest usable model wins: this is a sub-second fast path, and every
// suitable model has already cleared the same correctness bar.
const best = usable.reduce((a, b) => (a.latencyMs <= b.latencyMs ? a : b));
console.log(
  `\n  ${usable.length} of ${results.length} usable. Fastest: ${best.model} (${(best.latencyMs / 1000).toFixed(1)}s for 3 questions)\n\n` +
    `  Use it:\n    OLLAMA_MODEL=${best.model} node src/examples/routing_agent/demo.ts\n\n` +
    `  Note the latency. A local model is far slower than the 500ms System 1\n` +
    `  target, so it will time out against the default budget — by design. It\n` +
    `  is a fallback, and its confidence is capped so it can never\n` +
    `  auto-dispatch on its own say-so.\n`,
);
