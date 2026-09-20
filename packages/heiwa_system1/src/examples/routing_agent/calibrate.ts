/**
 * Runnable threshold calibration.
 *
 *   node src/examples/routing_agent/calibrate.ts
 *   TYPESAFE_API_KEY=sk-... node src/examples/routing_agent/calibrate.ts
 *   OLLAMA_MODEL=gemma4:latest node src/examples/routing_agent/calibrate.ts
 *
 * Runs the labelled corpus through whichever System 1 backend is configured,
 * sweeps candidate thresholds per question, and prints the trade-off table
 * plus the most permissive bar that still meets a zero-false-dispatch
 * budget.
 *
 * With no credentials it runs the offline simulator, which is keyword
 * heuristics and NOT a model — the workflow is real, the numbers are not.
 * The banner says so at runtime, because a calibration table is exactly the
 * kind of output that gets screenshotted out of context.
 */

import {
  type Adapter,
  ok,
  System1Client,
  structuredLlmAdapter,
  typesafeAdapter,
} from "../../core/system1/index.ts";
import {
  confidenceSpread,
  formatSweep,
  type LabelledSample,
  recommendThresholds,
  sweep,
} from "../../orchestrator/index.ts";
import { CORPUS } from "./corpus.ts";
import { INBOUND_QUESTIONS } from "./questions.ts";
import { simulateJev } from "./simulator.ts";

function buildAdapter(): {
  adapter: Adapter;
  label: string;
  trustworthy: boolean;
} {
  const typesafeKey = process.env.TYPESAFE_API_KEY;
  const ollamaModel = process.env.OLLAMA_MODEL;

  if (typesafeKey) {
    return {
      adapter: typesafeAdapter({ apiKey: typesafeKey }),
      label: "TypeSafe Jev",
      trustworthy: true,
    };
  }
  if (ollamaModel) {
    return {
      adapter: structuredLlmAdapter({
        model: ollamaModel,
        baseUrl: process.env.OLLAMA_URL ?? "http://127.0.0.1:11434",
      }),
      label: `Ollama ${ollamaModel} (uncalibrated fallback)`,
      trustworthy: true,
    };
  }
  return {
    adapter: {
      name: "simulator",
      async evaluate(req) {
        return ok(simulateJev(req.body.questions, req.body.state));
      },
    },
    label: "offline simulator",
    trustworthy: false,
  };
}

const { adapter, label, trustworthy } = buildAdapter();
const client = new System1Client({ adapter, budgetMs: 120_000 });

console.log(`\nThreshold calibration — backend: ${label}`);
if (!trustworthy) {
  console.log(
    "\n  ⚠  The offline simulator is keyword heuristics, NOT a model.\n" +
      "     The workflow below is real; the numbers are meaningless as evidence\n" +
      "     about Jev. Set TYPESAFE_API_KEY or OLLAMA_MODEL for real data.",
  );
}
console.log(
  `\nCorpus: ${CORPUS.length} labelled tickets (small — a demo, not a benchmark)\n`,
);

// Collect one System 1 answer set per labelled ticket.
const samples: LabelledSample[] = [];
let failures = 0;

for (const item of CORPUS) {
  const result = await client.evaluate({
    state: item.payload,
    questions: INBOUND_QUESTIONS,
  });
  if (!result.ok) {
    failures++;
    console.log(
      `  ! ${item.id}: ${result.error.kind} — ${result.error.message.slice(0, 100)}`,
    );
    continue;
  }
  samples.push({
    id: item.id,
    answers: result.value.answers,
    truth: item.truth,
  });
}

if (failures > 0)
  console.log(
    `\n  ${failures} of ${CORPUS.length} tickets failed to evaluate\n`,
  );

const QUESTIONS_TO_CALIBRATE = [
  "route",
  "injection",
  "pii",
  "automatable",
] as const;

for (const questionId of QUESTIONS_TO_CALIBRATE) {
  console.log(`\n${"─".repeat(64)}\n${questionId}\n`);
  // Check the signal before trusting the table. A capped adapter reports one
  // identical confidence for every sufficiently-confident answer, producing a
  // normal-looking sweep that discriminates nothing.
  const spread = confidenceSpread(samples, questionId);
  if (spread.degenerate) {
    console.log(
      `  !!  FLAT CONFIDENCE SIGNAL - ${spread.distinctValues} distinct value(s), ` +
        `${(spread.modeShare * 100).toFixed(0)}% on one.\n` +
        `      No threshold can separate right from wrong here. If this backend\n` +
        `      caps confidence (the structured-LLM fallback does), the cap has\n` +
        `      flattened the signal and the table below is not calibration.\n`,
    );
  }

  const points = sweep(samples, questionId, { steps: 11 });
  console.log(formatSweep(points));

  const rec = recommendThresholds(samples, questionId, {
    maxFalseAutoRate: 0,
    steps: 41,
  });
  if (!rec) {
    console.log(
      `\n  NO SAFE OPERATING POINT. Confidence does not separate right from wrong\n` +
        `  for "${questionId}" on this corpus. Split the question or separate the\n` +
        `  criteria — no threshold will fix this.`,
    );
    continue;
  }
  console.log(
    `\n  recommended auto ≥ ${rec.thresholds.auto.toFixed(3)}` +
      `  → ${(rec.metrics.autoRate * 100).toFixed(0)}% fast path,` +
      ` ${rec.metrics.falseAutoCount} wrong dispatches,` +
      ` ${(rec.metrics.quarantineRate * 100).toFixed(0)}% quarantined`,
  );
}

console.log(
  `\n${"─".repeat(64)}\n` +
    `Recommendations maximise fast-path throughput subject to ZERO wrong\n` +
    `auto-dispatches. Raise maxFalseAutoRate to trade safety for throughput.\n`,
);
