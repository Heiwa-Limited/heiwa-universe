import assert from "node:assert/strict";
import http from "node:http";
import type { AddressInfo } from "node:net";
import { after, before, describe, test } from "node:test";

import {
  FALLBACK_CONFIDENCE_CEILING,
  questionsToJsonSchema,
  structuredLlmAdapter,
} from "../../src/core/system1/adapters/structured_llm.ts";
import type { AdapterRequest } from "../../src/core/system1/adapters/types.ts";
import { DEFAULT_THRESHOLDS } from "../../src/orchestrator/gate.ts";

let server: http.Server;
let baseUrl: string;
let reply: (body: unknown) => unknown;
const received: unknown[] = [];

before(async () => {
  server = http.createServer((req, res) => {
    let raw = "";
    req.on("data", (c) => (raw += c));
    req.on("end", () => {
      const parsed = raw ? JSON.parse(raw) : null;
      received.push(parsed);
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify(reply(parsed)));
    });
  });
  await new Promise<void>((r) => server.listen(0, "127.0.0.1", r));
  baseUrl = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
});

after(async () => {
  await new Promise<void>((r) => server.close(() => r()));
});

/** Wrap a model answer object in an OpenAI-compatible chat completion. */
function completion(content: unknown) {
  return {
    choices: [
      { message: { role: "assistant", content: JSON.stringify(content) } },
    ],
    usage: { prompt_tokens: 300, completion_tokens: 40 },
  };
}

const request: AdapterRequest = {
  body: {
    state: "I was charged twice for order A-104",
    model: "jev-latest",
    questions: {
      team: {
        type: "choice",
        instructions: "Which team?",
        criteria: { billing: "money", technical: "bugs" },
      },
      urgency: {
        type: "score",
        instructions: "How urgent?",
        criteria: ["low", "mid", "high"],
      },
      refund: { type: "noul", instructions: "Refund requested?" },
    },
  },
};

function call(adapter: ReturnType<typeof structuredLlmAdapter>) {
  return adapter.evaluate(request, new AbortController().signal);
}

describe("questionsToJsonSchema()", () => {
  const schema = questionsToJsonSchema(request.body.questions);

  /** Read `schema.properties[id].properties[field]`, failing loudly if absent. */
  function prop(id: string, field: string): unknown {
    const props = schema.properties as Record<
      string,
      { properties?: Record<string, unknown> }
    >;
    const entry = props[id];
    assert.ok(entry, `schema has no property for question "${id}"`);
    assert.ok(entry.properties, `question "${id}" has no nested properties`);
    return entry.properties[field];
  }

  test("constrains a choice to an enum of exactly the offered keys", () => {
    assert.deepEqual((prop("team", "choice") as { enum: string[] }).enum, [
      "billing",
      "technical",
    ]);
  });

  test("constrains a score to the numeric span of the scale", () => {
    assert.deepEqual(prop("urgency", "score"), {
      type: "number",
      minimum: 0,
      maximum: 2,
    });
  });

  test("constrains a noul to a probability", () => {
    assert.deepEqual(prop("refund", "noul"), {
      type: "number",
      minimum: 0,
      maximum: 1,
    });
  });

  test("requires every question so the model cannot quietly skip one", () => {
    assert.deepEqual((schema.required as string[]).sort(), [
      "refund",
      "team",
      "urgency",
    ]);
  });

  test("forbids extra properties so the model cannot invent an answer", () => {
    assert.equal(schema.additionalProperties, false);
  });
});

describe("structuredLlmAdapter request", () => {
  test("sends the schema as a json_schema response format", async () => {
    reply = () =>
      completion({
        team: {
          choice: "billing",
          probabilities: { billing: 0.9, technical: 0.1 },
        },
        urgency: { score: 2, probabilities: [0.1, 0.1, 0.8] },
        refund: { noul: 0.9 },
      });
    received.length = 0;

    await call(structuredLlmAdapter({ baseUrl, model: "qwen3.5:9b" }));

    const sent = received[0] as {
      model: string;
      response_format: { type: string };
    };
    assert.equal(sent.model, "qwen3.5:9b");
    assert.equal(sent.response_format.type, "json_schema");
  });

  test("puts the state in the prompt so the model has something to judge", async () => {
    reply = () =>
      completion({
        team: { choice: "billing" },
        urgency: { score: 1 },
        refund: { noul: 0.5 },
      });
    received.length = 0;

    await call(structuredLlmAdapter({ baseUrl, model: "m" }));

    const sent = received[0] as { messages: Array<{ content: string }> };
    assert.ok(sent.messages.some((m) => m.content.includes("A-104")));
  });
});

describe("structuredLlmAdapter response translation", () => {
  test("produces a System One shaped body the ordinary decoder accepts", async () => {
    reply = () =>
      completion({
        team: {
          choice: "billing",
          probabilities: { billing: 0.9, technical: 0.1 },
        },
        urgency: { score: 2, probabilities: [0.1, 0.1, 0.8] },
        refund: { noul: 0.93 },
      });

    const r = await call(structuredLlmAdapter({ baseUrl, model: "m" }));
    assert.equal(r.ok, true);
    if (!r.ok) return;

    const body = r.value as {
      answers: Record<string, Record<string, unknown> | undefined>;
      usage: unknown;
    };
    assert.equal(body.answers.team?.choice, "billing");
    assert.deepEqual(body.answers.urgency?.legend, ["low", "mid", "high"]);
    assert.equal(body.answers.refund?.noul, 0.93);
    assert.deepEqual(body.usage, { input_tokens: 300, output_tokens: 40 });
  });

  test("fills a uniform distribution when the model supplied none", async () => {
    reply = () =>
      completion({
        team: { choice: "billing" },
        urgency: { score: 1 },
        refund: { noul: 0.5 },
      });

    const r = await call(structuredLlmAdapter({ baseUrl, model: "m" }));
    assert.equal(r.ok, true);
    if (!r.ok) return;

    const team = (
      r.value as {
        answers: Record<
          string,
          { probabilities: Record<string, number> } | undefined
        >;
      }
    ).answers.team;
    assert.ok(team, "the translated body must carry a team answer");
    assert.deepEqual(Object.keys(team.probabilities).sort(), [
      "billing",
      "technical",
    ]);
  });
});

describe("structuredLlmAdapter confidence honesty", () => {
  test("caps confidence below the auto threshold, because an LLM is not calibrated", async () => {
    reply = () =>
      completion({
        team: {
          choice: "billing",
          probabilities: { billing: 1, technical: 0 },
          confidence: 1,
        },
        urgency: { score: 2, confidence: 1 },
        refund: { noul: 1 },
      });

    const r = await call(structuredLlmAdapter({ baseUrl, model: "m" }));
    assert.equal(r.ok, true);
    if (!r.ok) return;

    const answers = (
      r.value as { answers: Record<string, { confidence?: number }> }
    ).answers;
    assert.equal(answers.team?.confidence, FALLBACK_CONFIDENCE_CEILING);
    assert.equal(answers.urgency?.confidence, FALLBACK_CONFIDENCE_CEILING);
  });

  test("the ceiling sits below the auto band, so a fallback can never auto-dispatch by default", () => {
    assert.ok(
      FALLBACK_CONFIDENCE_CEILING <= DEFAULT_THRESHOLDS.auto,
      "a degraded adapter must not be able to open the fast path",
    );
    assert.ok(FALLBACK_CONFIDENCE_CEILING >= DEFAULT_THRESHOLDS.deliberate);
  });

  test("an operator can raise the ceiling deliberately, but must do so explicitly", async () => {
    reply = () =>
      completion({
        team: { choice: "billing", confidence: 0.99 },
        urgency: { score: 1 },
        refund: { noul: 1 },
      });

    const r = await call(
      structuredLlmAdapter({ baseUrl, model: "m", confidenceCeiling: 0.95 }),
    );
    assert.equal(r.ok, true);
    if (!r.ok) return;
    assert.equal(
      (r.value as { answers: Record<string, { confidence: number }> }).answers
        .team?.confidence,
      0.95,
    );
  });

  test("keeps a low self-reported confidence rather than raising it to the ceiling", async () => {
    reply = () =>
      completion({
        team: { choice: "billing", confidence: 0.2 },
        urgency: { score: 1 },
        refund: { noul: 1 },
      });

    const r = await call(structuredLlmAdapter({ baseUrl, model: "m" }));
    assert.equal(r.ok, true);
    if (!r.ok) return;
    assert.equal(
      (r.value as { answers: Record<string, { confidence: number }> }).answers
        .team?.confidence,
      0.2,
    );
  });
});

describe("structuredLlmAdapter failure handling", () => {
  test("turns a model that emitted non-JSON content into a schema violation", async () => {
    reply = () => ({
      choices: [{ message: { content: "I think it's billing, probably?" } }],
    });

    const r = await call(structuredLlmAdapter({ baseUrl, model: "m" }));
    assert.equal(r.ok, false);
    if (r.ok) return;
    assert.equal(r.error.kind, "schema_violation");
  });

  test("turns a completion with no choices into a schema violation", async () => {
    reply = () => ({ choices: [] });

    const r = await call(structuredLlmAdapter({ baseUrl, model: "m" }));
    assert.equal(r.ok, false);
    if (r.ok) return;
    assert.equal(r.error.kind, "schema_violation");
  });

  test("is named distinctly so telemetry shows a degraded decision", () => {
    assert.match(
      structuredLlmAdapter({ baseUrl, model: "qwen3.5:9b" }).name,
      /structured-llm/,
    );
  });
});

describe("structuredLlmAdapter output budget", () => {
  // Found against real Ollama: a reasoning model with no max_tokens spent
  // 3745 completion tokens on chain-of-thought, hit the context limit, and
  // returned an EMPTY content field after 75 seconds. An unbounded output
  // budget is not a default, it is a hang.
  test("caps the output budget by default rather than letting a model run away", async () => {
    reply = () =>
      completion({
        team: { choice: "billing" },
        urgency: { score: 1 },
        refund: { noul: 0.5 },
      });
    received.length = 0;

    await call(structuredLlmAdapter({ baseUrl, model: "m" }));

    const sent = received[0] as { max_tokens?: number };
    assert.ok(
      typeof sent.max_tokens === "number",
      "max_tokens must always be sent",
    );
    assert.ok(sent.max_tokens > 0);
  });

  test("scales the default budget with the number of questions", async () => {
    reply = () =>
      completion({
        team: { choice: "billing" },
        urgency: { score: 1 },
        refund: { noul: 0.5 },
      });

    received.length = 0;
    await call(structuredLlmAdapter({ baseUrl, model: "m" }));
    const forThree = (received[0] as { max_tokens: number }).max_tokens;

    received.length = 0;
    const one: AdapterRequest = {
      body: {
        state: "x",
        model: "m",
        questions: { only: { type: "noul", instructions: "?" } },
      },
    };
    await structuredLlmAdapter({ baseUrl, model: "m" }).evaluate(
      one,
      new AbortController().signal,
    );
    const forOne = (received[0] as { max_tokens: number }).max_tokens;

    assert.ok(
      forThree > forOne,
      `${forThree} should exceed ${forOne} for more questions`,
    );
  });

  test("leaves enough headroom that a model which reasons a little still answers", async () => {
    // Measured: gemma4 emitted ~3480 chars (~870 tokens) of chain-of-thought
    // on the harder corpus tickets before answering, and a 832-token cap cut
    // it off — 2 of 16 tickets lost. The token cap is a backstop against a
    // pathological model; the AbortController latency budget is the real
    // control. So the cap must be generous enough not to manufacture
    // failures on ordinary inputs.
    reply = () =>
      completion({
        team: { choice: "billing" },
        urgency: { score: 1 },
        refund: { noul: 0.5 },
      });
    received.length = 0;

    await call(structuredLlmAdapter({ baseUrl, model: "m" }));

    const sent = received[0] as { max_tokens: number };
    assert.ok(
      sent.max_tokens >= 1024,
      `default of ${sent.max_tokens} is too tight for a model that reasons before answering`,
    );
  });

  test("honours an explicit maxTokens", async () => {
    reply = () =>
      completion({
        team: { choice: "billing" },
        urgency: { score: 1 },
        refund: { noul: 0.5 },
      });
    received.length = 0;

    await call(structuredLlmAdapter({ baseUrl, model: "m", maxTokens: 77 }));

    assert.equal((received[0] as { max_tokens: number }).max_tokens, 77);
  });
});

describe("structuredLlmAdapter diagnoses truncation distinctly", () => {
  // "not JSON" is the wrong diagnosis for a model that never got to answer.
  // The operator's fix differs: raise max_tokens or switch model, versus
  // fix the schema. The error message has to say which.
  test("names truncation when the model hit the output budget with nothing to show", async () => {
    reply = () => ({
      choices: [
        {
          finish_reason: "length",
          message: { role: "assistant", content: "" },
        },
      ],
      usage: { prompt_tokens: 351, completion_tokens: 512 },
    });

    const r = await call(structuredLlmAdapter({ baseUrl, model: "m" }));
    assert.equal(r.ok, false);
    if (r.ok) return;
    assert.equal(r.error.kind, "schema_violation");
    assert.match(r.error.message, /truncat|output budget|max_tokens/i);
    assert.doesNotMatch(
      r.error.message,
      /not JSON/i,
      "truncation is not a JSON syntax problem",
    );
  });

  test("names chain-of-thought when a reasoning model returned only reasoning", async () => {
    reply = () => ({
      choices: [
        {
          finish_reason: "length",
          message: {
            role: "assistant",
            content: "",
            reasoning: "Thinking Process: ...".repeat(50),
          },
        },
      ],
      usage: { prompt_tokens: 351, completion_tokens: 3745 },
    });

    const r = await call(structuredLlmAdapter({ baseUrl, model: "m" }));
    assert.equal(r.ok, false);
    if (r.ok) return;
    assert.match(
      r.error.message,
      /reasoning/i,
      "must point at the actual cause",
    );
  });

  test("still reports a genuine syntax error as one", async () => {
    reply = () => ({
      choices: [
        {
          finish_reason: "stop",
          message: { content: "I think it's billing, probably?" },
        },
      ],
      usage: {},
    });

    const r = await call(structuredLlmAdapter({ baseUrl, model: "m" }));
    assert.equal(r.ok, false);
    if (r.ok) return;
    assert.match(r.error.message, /not JSON/i);
  });
});

describe("structuredLlmAdapter does not repair out-of-range values", () => {
  // Found against real Ollama: its json_schema constraint enforces object
  // shape and `enum`, but IGNORES `minimum`/`maximum`. A real run produced
  // noul: 2, noul: 4 and score: -0.8. Clamping those would convert "the
  // model returned nonsense" into "we confidently made an answer up", so
  // they are passed through for the decoder to reject and quarantine.
  test("passes an out-of-range noul through so the decoder can reject it", async () => {
    reply = () =>
      completion({
        team: { choice: "billing" },
        urgency: { score: 1 },
        refund: { noul: 4 },
      });

    const r = await call(structuredLlmAdapter({ baseUrl, model: "m" }));
    assert.equal(r.ok, true, "the adapter itself does not judge values");
    if (!r.ok) return;

    const refund = (
      r.value as { answers: Record<string, { noul?: number } | undefined> }
    ).answers.refund;
    assert.equal(refund?.noul, 4, "must not be silently clamped to 1");
  });

  test("passes a negative score through unrepaired", async () => {
    reply = () =>
      completion({
        team: { choice: "billing" },
        urgency: { score: -0.8 },
        refund: { noul: 0.5 },
      });

    const r = await call(structuredLlmAdapter({ baseUrl, model: "m" }));
    assert.equal(r.ok, true);
    if (!r.ok) return;
    const urgency = (
      r.value as { answers: Record<string, { score?: number } | undefined> }
    ).answers.urgency;
    assert.equal(urgency?.score, -0.8);
  });
});
