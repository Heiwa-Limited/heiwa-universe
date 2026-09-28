import assert from "node:assert/strict";
import { describe, test } from "node:test";
import type { Adapter } from "../../src/core/system1/adapters/types.ts";
import {
  MAX_QUESTIONS_PER_REQUEST,
  System1Client,
} from "../../src/core/system1/client.ts";
import { err, ok, system1Error } from "../../src/core/system1/errors.ts";
import type { Question } from "../../src/core/system1/primitives.ts";
import { noul } from "../../src/core/system1/primitives.ts";
import { speculate } from "../../src/orchestrator/speculative.ts";

function questionSet(n: number): Record<string, Question> {
  const q: Record<string, Question> = {};
  for (let i = 0; i < n; i++)
    q[`q${i}`] = noul({ instructions: `question ${i}` });
  return q;
}

/** Answers every noul it is asked, after `delayMs`, tracking concurrency. */
function countingAdapter(delayMs = 0) {
  const state = {
    inFlight: 0,
    maxInFlight: 0,
    requests: 0,
    questionCounts: [] as number[],
  };
  const adapter: Adapter = {
    name: "counting",
    async evaluate(req) {
      state.requests++;
      state.inFlight++;
      state.maxInFlight = Math.max(state.maxInFlight, state.inFlight);
      const ids = Object.keys(req.body.questions);
      state.questionCounts.push(ids.length);
      if (delayMs > 0) await new Promise((r) => setTimeout(r, delayMs));
      state.inFlight--;
      const answers: Record<string, unknown> = {};
      for (const id of ids) answers[id] = { noul: 0.9 };
      return ok({
        model: "jev-latest",
        answers,
        usage: { input_tokens: 5, output_tokens: 1 },
      });
    },
  };
  return { adapter, state };
}

describe("speculate() batching", () => {
  test("sends a single request when the fan fits in one", async () => {
    const { adapter, state } = countingAdapter();
    const client = new System1Client({ adapter });

    const r = await speculate(client, {
      state: "x",
      questions: questionSet(20),
    });

    assert.equal(state.requests, 1);
    assert.equal(Object.keys(r.answers).length, 20);
  });

  test("splits a fan larger than one request into chunks at the cap", async () => {
    const { adapter, state } = countingAdapter();
    const client = new System1Client({ adapter });

    await speculate(client, { state: "x", questions: questionSet(45) });

    assert.equal(state.requests, 3);
    assert.deepEqual(
      state.questionCounts.sort((a, b) => b - a),
      [20, 20, 5],
    );
    assert.ok(
      state.questionCounts.every((c) => c <= MAX_QUESTIONS_PER_REQUEST),
    );
  });

  test("dispatches chunks concurrently rather than one after another", async () => {
    const { adapter, state } = countingAdapter(30);
    const client = new System1Client({ adapter, budgetMs: 2000 });

    await speculate(client, { state: "x", questions: questionSet(45) });

    assert.equal(
      state.maxInFlight,
      3,
      "all three chunks must be in flight together",
    );
  });

  test("merges every chunk's answers into one keyed map", async () => {
    const { adapter } = countingAdapter();
    const client = new System1Client({ adapter });

    const r = await speculate(client, {
      state: "x",
      questions: questionSet(45),
    });

    assert.equal(Object.keys(r.answers).length, 45);
    assert.equal(r.answers.q0?.kind, "noul");
    assert.equal(r.answers.q44?.kind, "noul");
  });

  test("sums token usage and reports the slowest chunk as the wall-clock latency", async () => {
    const { adapter } = countingAdapter();
    const client = new System1Client({ adapter });

    const r = await speculate(client, {
      state: "x",
      questions: questionSet(45),
    });

    assert.equal(r.usage.inputTokens, 15);
    assert.equal(r.requests, 3);
    assert.ok(r.latencyMs >= 0);
  });
});

describe("speculate() partial failure", () => {
  test("keeps the answers that did arrive and reports the questions that did not", async () => {
    let call = 0;
    const adapter: Adapter = {
      name: "flaky",
      async evaluate(req) {
        call++;
        if (call === 2) {
          return err(
            system1Error("provider_unavailable", "chunk 2 is down", {
              adapter: "flaky",
            }),
          );
        }
        const answers: Record<string, unknown> = {};
        for (const id of Object.keys(req.body.questions))
          answers[id] = { noul: 0.9 };
        return ok({
          model: "jev-latest",
          answers,
          usage: { input_tokens: 5, output_tokens: 1 },
        });
      },
    };
    const client = new System1Client({ adapter });

    const r = await speculate(client, {
      state: "x",
      questions: questionSet(45),
    });

    assert.equal(r.failures.length, 1);
    assert.equal(r.failures[0]?.error.kind, "provider_unavailable");
    assert.equal(r.failures[0]?.questionIds.length, 20);
    assert.equal(Object.keys(r.answers).length, 25);
    assert.equal(r.complete, false);
  });

  test("reports complete when every chunk answered", async () => {
    const { adapter } = countingAdapter();
    const client = new System1Client({ adapter });
    const r = await speculate(client, {
      state: "x",
      questions: questionSet(45),
    });

    assert.equal(r.complete, true);
    assert.deepEqual(r.failures, []);
  });

  test("never throws when every chunk fails", async () => {
    const adapter: Adapter = {
      name: "dead",
      async evaluate() {
        return err(
          system1Error("transport", "nothing is listening", {
            adapter: "dead",
          }),
        );
      },
    };
    const client = new System1Client({ adapter });

    const r = await speculate(client, {
      state: "x",
      questions: questionSet(45),
    });

    assert.equal(r.complete, false);
    assert.equal(r.failures.length, 3);
    assert.deepEqual(Object.keys(r.answers), []);
  });

  test("handles an empty fan without dispatching anything", async () => {
    const { adapter, state } = countingAdapter();
    const client = new System1Client({ adapter });

    const r = await speculate(client, { state: "x", questions: {} });

    assert.equal(state.requests, 0);
    assert.equal(r.complete, true);
    assert.deepEqual(Object.keys(r.answers), []);
    // Regression: the empty-fan branch once omitted `missing` entirely. No
    // runtime assertion noticed, because reading an absent field just gives
    // undefined; only the compiler did. Pin every field of the contract.
    assert.deepEqual(r.missing, []);
    assert.deepEqual(r.failures, []);
    assert.deepEqual(r.usage, { inputTokens: 0, outputTokens: 0 });
  });

  test("returns a null-prototype answer map on every path, so no inherited key can pose as an answer", async () => {
    const { adapter } = countingAdapter();
    const client = new System1Client({ adapter });

    const populated = await speculate(client, {
      state: "x",
      questions: questionSet(3),
    });
    const empty = await speculate(client, { state: "x", questions: {} });

    assert.equal(Object.getPrototypeOf(populated.answers), null);
    assert.equal(Object.getPrototypeOf(empty.answers), null);
    assert.equal(
      (populated.answers as Record<string, unknown>).toString,
      undefined,
    );
  });
});
