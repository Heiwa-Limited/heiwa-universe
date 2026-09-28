import assert from "node:assert/strict";
import { describe, test } from "node:test";
import type {
  Adapter,
  AdapterRequest,
} from "../../src/core/system1/adapters/types.ts";
import {
  MAX_QUESTIONS_PER_REQUEST,
  System1Client,
} from "../../src/core/system1/client.ts";
import { err, ok, system1Error } from "../../src/core/system1/errors.ts";
import { choice, noul } from "../../src/core/system1/primitives.ts";

const questions = {
  team: choice({
    instructions: "Which team?",
    criteria: { billing: "money", technical: "bugs" },
  }),
  refund: noul({ instructions: "Refund requested?" }),
};

const goodBody = {
  model: "jev-latest",
  answers: {
    team: {
      choice: "billing",
      probabilities: { billing: 0.9, technical: 0.1 },
      confidence: 0.8,
    },
    refund: { noul: 0.92 },
  },
  usage: { input_tokens: 100, output_tokens: 10 },
};

/** A real Adapter implementation, driven by a supplied behaviour. */
function fakeAdapter(
  behaviour: (req: AdapterRequest, signal: AbortSignal) => Promise<unknown>,
  name = "fake",
): Adapter & { calls: AdapterRequest[] } {
  const calls: AdapterRequest[] = [];
  return {
    name,
    calls,
    async evaluate(req, signal) {
      calls.push(req);
      try {
        return ok(await behaviour(req, signal));
      } catch (cause) {
        return err(
          system1Error("transport", String(cause), { adapter: name, cause }),
        );
      }
    },
  };
}

describe("System1Client envelope", () => {
  test("sends state, model and a questions map of raw wire shapes", async () => {
    const adapter = fakeAdapter(async () => goodBody);
    const client = new System1Client({ adapter, model: "jev-latest" });

    await client.evaluate({ state: "I want a refund", questions });

    assert.equal(adapter.calls.length, 1);
    assert.deepEqual(adapter.calls[0]?.body, {
      state: "I want a refund",
      model: "jev-latest",
      questions: {
        team: {
          type: "choice",
          instructions: "Which team?",
          criteria: { billing: "money", technical: "bugs" },
        },
        refund: { type: "noul", instructions: "Refund requested?" },
      },
    });
  });

  test("passes a structured state object through unchanged", async () => {
    const adapter = fakeAdapter(async () => goodBody);
    const client = new System1Client({ adapter });
    const state = {
      ticket: { subject: "Duplicate charge" },
      order: { id: "A-104" },
    };

    await client.evaluate({ state, questions });

    assert.deepEqual(
      (adapter.calls[0]?.body as { state: unknown }).state,
      state,
    );
  });
});

describe("System1Client success path", () => {
  test("returns decoded answers keyed by question id", async () => {
    const client = new System1Client({
      adapter: fakeAdapter(async () => goodBody),
    });
    const r = await client.evaluate({ state: "x", questions });

    assert.equal(r.ok, true);
    if (!r.ok) return;
    assert.equal(r.value.answers.team?.kind, "choice");
    assert.equal(r.value.answers.refund?.kind, "noul");
  });

  test("reports observed latency and token usage", async () => {
    const client = new System1Client({
      adapter: fakeAdapter(async () => goodBody),
    });
    const r = await client.evaluate({ state: "x", questions });

    assert.equal(r.ok, true);
    if (!r.ok) return;
    assert.ok(r.value.latencyMs >= 0);
    assert.deepEqual(r.value.usage, { inputTokens: 100, outputTokens: 10 });
    assert.equal(r.value.adapter, "fake");
  });
});

describe("System1Client never throws", () => {
  test("turns an adapter that rejects into a typed transport failure", async () => {
    const adapter = fakeAdapter(async () => {
      throw new Error("socket hang up");
    });
    const client = new System1Client({ adapter });

    const r = await client.evaluate({ state: "x", questions });
    assert.equal(r.ok, false);
    if (r.ok) return;
    assert.equal(r.error.kind, "transport");
  });

  test("turns a garbage provider body into a typed schema violation", async () => {
    const client = new System1Client({
      adapter: fakeAdapter(async () => "<html>502</html>"),
    });

    const r = await client.evaluate({ state: "x", questions });
    assert.equal(r.ok, false);
    if (r.ok) return;
    assert.equal(r.error.kind, "schema_violation");
  });

  test("turns an adapter that throws synchronously into a typed failure", async () => {
    const hostile: Adapter = {
      name: "hostile",
      evaluate() {
        throw new Error("adapter is broken");
      },
    };
    const client = new System1Client({ adapter: hostile });

    const r = await client.evaluate({ state: "x", questions });
    assert.equal(r.ok, false);
    if (r.ok) return;
    assert.equal(r.error.kind, "transport");
  });
});

describe("System1Client latency budget", () => {
  test("aborts and returns a timeout once the budget is spent", async () => {
    const adapter = fakeAdapter(
      (_req, signal) =>
        new Promise((_resolve, reject) => {
          signal.addEventListener("abort", () => reject(new Error("aborted")), {
            once: true,
          });
        }),
    );
    const client = new System1Client({ adapter, budgetMs: 20 });

    const started = Date.now();
    const r = await client.evaluate({ state: "x", questions });

    assert.equal(r.ok, false);
    if (r.ok) return;
    assert.equal(r.error.kind, "timeout");
    assert.ok(Date.now() - started < 1000, "must not wait past the budget");
  });

  test("defaults the budget to the documented 500ms target", () => {
    assert.equal(
      new System1Client({ adapter: fakeAdapter(async () => goodBody) })
        .budgetMs,
      500,
    );
  });
});

describe("System1Client request cap", () => {
  test("refuses more questions than one request is allowed to carry", async () => {
    const many: Record<string, ReturnType<typeof noul>> = {};
    for (let i = 0; i <= MAX_QUESTIONS_PER_REQUEST; i++) {
      many[`q${i}`] = noul({ instructions: `question ${i}` });
    }
    const client = new System1Client({
      adapter: fakeAdapter(async () => goodBody),
    });

    const r = await client.evaluate({ state: "x", questions: many });
    assert.equal(r.ok, false);
    if (r.ok) return;
    assert.equal(r.error.kind, "invalid_request");
    assert.match(r.error.message, /20/);
  });

  test("refuses an empty question set rather than paying for a null request", async () => {
    const client = new System1Client({
      adapter: fakeAdapter(async () => goodBody),
    });
    const r = await client.evaluate({ state: "x", questions: {} });
    assert.equal(r.ok, false);
  });
});
