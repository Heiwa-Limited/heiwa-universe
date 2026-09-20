/**
 * Live endpoint smoke test. SKIPPED unless a key is present.
 *
 *   TYPESAFE_API_KEY=sk-... npm run test:live
 *   OPENROUTER_API_KEY=or-... npm run test:live     # also checks that route
 *
 * This is the one gap the rest of the suite cannot close: everything else
 * runs against the documented contract and local servers. Only this file
 * proves the contract matches the real endpoint, and only this file measures
 * real latency and real calibration.
 *
 * It asserts the CONTRACT, never a specific answer. A model is allowed to
 * disagree with us about a ticket; it is not allowed to return a shape we
 * cannot decode or to blow the latency budget.
 */

import assert from "node:assert/strict";
import { describe, test } from "node:test";

import {
  type Adapter,
  memorySink,
  openRouterAdapter,
  System1Client,
  typesafeAdapter,
} from "../../src/core/system1/index.ts";
import { INBOUND_QUESTIONS } from "../../src/examples/routing_agent/questions.ts";

const TYPESAFE_KEY = process.env.TYPESAFE_API_KEY;
const OPENROUTER_KEY = process.env.OPENROUTER_API_KEY;

const TICKET = {
  channel: "email",
  from: "customer@example.com",
  subject: "Duplicate charge on order A-104",
  message:
    "I was charged twice for order A-104. Please refund the duplicate charge.",
};

function contractChecks(
  label: string,
  makeAdapter: () => Adapter,
  budgetMs: number,
) {
  describe(`live: ${label}`, () => {
    test("answers the whole fan in one request, inside the budget", async () => {
      const telemetry = memorySink();
      const client = new System1Client({
        adapter: makeAdapter(),
        budgetMs,
        telemetry,
      });

      const result = await client.evaluate({
        state: TICKET,
        questions: INBOUND_QUESTIONS,
      });

      if (!result.ok) {
        assert.fail(
          `live call failed: ${result.error.kind} — ${result.error.message}`,
        );
      }

      // Contract, not content.
      assert.deepEqual(
        Object.keys(result.value.answers).sort(),
        Object.keys(INBOUND_QUESTIONS).sort(),
        "every question must come back answered",
      );
      assert.deepEqual(result.value.missing, []);
      assert.ok(result.value.usage.inputTokens > 0, "usage must be reported");

      const record = telemetry.records[0];
      assert.ok(record);
      console.log(
        `\n  ${label}: ${result.value.latencyMs}ms · ${result.value.usage.inputTokens} in / ${result.value.usage.outputTokens} out · model=${result.value.model}`,
      );
      for (const q of record.questions) {
        console.log(
          `    ${q.id.padEnd(12)} ${q.kind.padEnd(7)} conf=${q.confidence.toFixed(3)} (${q.confidenceSource})`,
        );
      }

      assert.ok(
        result.value.latencyMs < budgetMs,
        `latency ${result.value.latencyMs}ms exceeded the ${budgetMs}ms budget`,
      );
    });

    test("returns probabilities that are well-formed distributions", async () => {
      const client = new System1Client({ adapter: makeAdapter(), budgetMs });
      const result = await client.evaluate({
        state: TICKET,
        questions: INBOUND_QUESTIONS,
      });
      if (!result.ok) assert.fail(`live call failed: ${result.error.message}`);

      for (const [id, answer] of Object.entries(result.value.answers)) {
        if (answer.kind === "choice") {
          const total = Object.values(answer.probabilities).reduce(
            (a, b) => a + b,
            0,
          );
          assert.ok(
            Math.abs(total - 1) < 0.02,
            `${id}: probabilities sum to ${total}`,
          );
          assert.ok(
            Object.hasOwn(answer.probabilities, answer.choice),
            `${id}: chose an option absent from its own distribution`,
          );
        }
        if (answer.kind === "score") {
          const total = answer.probabilities.reduce((a, b) => a + b, 0);
          assert.ok(
            Math.abs(total - 1) < 0.02,
            `${id}: probabilities sum to ${total}`,
          );
        }
      }
    });
  });
}

if (TYPESAFE_KEY) {
  contractChecks(
    "typesafe /v1/systemone",
    () => typesafeAdapter({ apiKey: TYPESAFE_KEY }),
    1500,
  );
} else {
  test("live: typesafe — skipped (set TYPESAFE_API_KEY to run)", {
    skip: true,
  }, () => {});
}

if (OPENROUTER_KEY) {
  // The OpenRouter route is assembled from community sources; this is the
  // check that confirms or refutes it. A failure here means the constants in
  // adapters/http.ts need correcting, not that the engine is broken.
  contractChecks(
    "openrouter /api/alpha/decisions",
    () => openRouterAdapter({ apiKey: OPENROUTER_KEY }),
    2000,
  );
} else {
  test("live: openrouter — skipped (set OPENROUTER_API_KEY to run)", {
    skip: true,
  }, () => {});
}
