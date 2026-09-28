import assert from "node:assert/strict";
import { describe, test } from "node:test";
import type { Adapter } from "../../src/core/system1/adapters/types.ts";
import { System1Client } from "../../src/core/system1/client.ts";
import { err, ok, system1Error } from "../../src/core/system1/errors.ts";
import { choice, noul } from "../../src/core/system1/primitives.ts";
import { Pipeline } from "../../src/orchestrator/pipeline.ts";

const questions = {
  team: choice({
    instructions: "Which team?",
    criteria: { billing: "money", technical: "bugs" },
  }),
  safe: noul({ instructions: "Is this safe to automate?" }),
};

/** Serves a fixed answer body, so gating behaviour is fully deterministic. */
function servingAdapter(answers: Record<string, unknown>): Adapter {
  return {
    name: "serving",
    async evaluate() {
      return ok({
        model: "jev-latest",
        answers,
        usage: { input_tokens: 80, output_tokens: 6 },
      });
    },
  };
}

function confident() {
  return servingAdapter({
    team: {
      choice: "billing",
      probabilities: { billing: 0.97, technical: 0.03 },
      confidence: 0.95,
    },
    safe: { noul: 0.99 },
  });
}

function middling() {
  return servingAdapter({
    team: {
      choice: "billing",
      probabilities: { billing: 0.7, technical: 0.3 },
      confidence: 0.7,
    },
    safe: { noul: 0.9 },
  });
}

function unsure() {
  return servingAdapter({
    team: {
      choice: "billing",
      probabilities: { billing: 0.4, technical: 0.35 },
      confidence: 0.3,
    },
    safe: { noul: 0.52 },
  });
}

type Calls = { dispatch: number; system2: number; quarantine: number };

function pipelineWith(
  adapter: Adapter,
  overrides: Record<string, unknown> = {},
) {
  const calls: Calls = { dispatch: 0, system2: 0, quarantine: 0 };
  const quarantined: unknown[] = [];
  const pipeline = new Pipeline({
    client: new System1Client({ adapter }),
    questions,
    async dispatch(ctx) {
      calls.dispatch++;
      return `dispatched:${(ctx.answers.team as { choice: string }).choice}`;
    },
    async system2() {
      calls.system2++;
      return "deliberated";
    },
    async quarantine(record) {
      calls.quarantine++;
      quarantined.push(record);
    },
    ...overrides,
  });
  return { pipeline, calls, quarantined };
}

describe("Pipeline routing by confidence band", () => {
  test("dispatches directly when every gating judgment is confident", async () => {
    const { pipeline, calls } = pipelineWith(confident());
    const outcome = await pipeline.run("customer wants a refund");

    assert.equal(outcome.route, "dispatched");
    assert.deepEqual(calls, { dispatch: 1, system2: 0, quarantine: 0 });
    assert.equal(outcome.ok, true);
    if (!outcome.ok) return;
    assert.equal(outcome.value, "dispatched:billing");
  });

  test("escalates to System 2 in the middle band", async () => {
    const { pipeline, calls } = pipelineWith(middling());
    const outcome = await pipeline.run("x");

    assert.equal(outcome.route, "deliberated");
    assert.deepEqual(calls, { dispatch: 0, system2: 1, quarantine: 0 });
  });

  test("quarantines below the deliberate threshold and never calls a handler", async () => {
    const { pipeline, calls } = pipelineWith(unsure());
    const outcome = await pipeline.run("x");

    assert.equal(outcome.route, "quarantined");
    assert.deepEqual(calls, { dispatch: 0, system2: 0, quarantine: 1 });
    assert.equal(outcome.ok, false);
  });
});

describe("Pipeline quarantine record", () => {
  test("carries the gate decisions, blockers and telemetry a reviewer needs", async () => {
    const { pipeline, quarantined } = pipelineWith(unsure());
    await pipeline.run("the offending payload");

    const record = quarantined[0] as Record<string, unknown>;
    assert.equal(record.state, "the offending payload");
    assert.equal((record.gate as { band: string }).band, "quarantine");
    assert.ok(
      (record.gate as { blockers: string[] }).blockers.includes("team"),
    );
    assert.ok(typeof record.latencyMs === "number");
    assert.deepEqual(record.usage, { inputTokens: 80, outputTokens: 6 });
    assert.ok(Array.isArray(record.failures));
    assert.ok((record.answers as Record<string, unknown>).team);
  });
});

describe("Pipeline fails closed", () => {
  test("quarantines rather than dispatching when a gating question got no answer", async () => {
    const dead: Adapter = {
      name: "dead",
      async evaluate() {
        return err(
          system1Error("provider_unavailable", "down", { adapter: "dead" }),
        );
      },
    };
    const { pipeline, calls, quarantined } = pipelineWith(dead);
    const outcome = await pipeline.run("x");

    assert.equal(outcome.route, "quarantined");
    assert.deepEqual(calls, { dispatch: 0, system2: 0, quarantine: 1 });
    const record = quarantined[0] as { unanswered: string[] };
    assert.deepEqual(record.unanswered.sort(), ["safe", "team"]);
  });

  test("quarantines when System 1 returned a body that violated the schema", async () => {
    const garbage: Adapter = {
      name: "garbage",
      async evaluate() {
        return ok("<html>502 Bad Gateway</html>");
      },
    };
    const { pipeline, calls } = pipelineWith(garbage);
    const outcome = await pipeline.run("x");

    assert.equal(outcome.route, "quarantined");
    assert.equal(calls.dispatch, 0);
  });

  test("still dispatches when only an informational question is missing", async () => {
    const partial: Adapter = {
      name: "partial",
      async evaluate() {
        return ok({
          model: "jev-latest",
          answers: {
            team: {
              choice: "billing",
              probabilities: { billing: 0.97, technical: 0.03 },
              confidence: 0.95,
            },
            safe: { noul: 0.99 },
            // `mood` is asked but never answered
          },
          usage: { input_tokens: 10, output_tokens: 1 },
        });
      },
    };
    const calls = { dispatch: 0 };
    const pipeline = new Pipeline({
      client: new System1Client({ adapter: partial }),
      questions: { ...questions, mood: noul({ instructions: "Happy?" }) },
      policies: { mood: { informational: true } },
      async dispatch() {
        calls.dispatch++;
        return "ok";
      },
      async system2() {
        return "s2";
      },
      async quarantine() {},
    });

    const outcome = await pipeline.run("x");
    assert.equal(outcome.route, "dispatched");
    assert.equal(calls.dispatch, 1);
  });
});

describe("Pipeline generation escalation", () => {
  test("routes to System 2 even at high confidence when generation is required", async () => {
    const { pipeline, calls } = pipelineWith(confident(), {
      requiresGeneration: () => true,
    });
    const outcome = await pipeline.run("write me a long apology");

    assert.equal(outcome.route, "deliberated");
    assert.deepEqual(calls, { dispatch: 0, system2: 1, quarantine: 0 });
  });
});

describe("Pipeline never throws", () => {
  test("converts a throwing dispatch handler into a typed failure", async () => {
    const { pipeline } = pipelineWith(confident(), {
      dispatch: async () => {
        throw new Error("the microservice is on fire");
      },
    });

    const outcome = await pipeline.run("x");
    assert.equal(outcome.ok, false);
    if (outcome.ok) return;
    assert.match(outcome.error.message, /on fire/);
  });

  test("survives a throwing quarantine sink without losing the verdict", async () => {
    const { pipeline } = pipelineWith(unsure(), {
      quarantine: async () => {
        throw new Error("review queue unreachable");
      },
    });

    const outcome = await pipeline.run("x");
    assert.equal(outcome.route, "quarantined");
  });
});
