import assert from "node:assert/strict";
import { describe, test } from "node:test";

import { choice, noul, score } from "../../src/core/system1/primitives.ts";
import { decodeResponse } from "../../src/core/system1/schema.ts";

const questions = {
  team: choice({
    instructions: "Which team?",
    criteria: { billing: "money", technical: "bugs" },
  }),
  urgency: score({
    instructions: "How urgent?",
    criteria: ["low", "mid", "high"],
  }),
  refund: noul({ instructions: "Refund requested?" }),
};

const goodBody = {
  model: "jev-latest",
  answers: {
    team: {
      choice: "billing",
      probabilities: { billing: 0.85, technical: 0.15 },
      confidence: 0.7,
    },
    urgency: {
      score: 1.5,
      legend: { "0": "low", "1": "mid", "2": "high" },
      probabilities: { "0": 0.2, "1": 0.6, "2": 0.2 },
      confidence: 0.6,
    },
    refund: { noul: 0.92 },
  },
  usage: { input_tokens: 412, output_tokens: 33 },
};

/**
 * Verbatim response examples from TypeSafe's API reference
 * (docs.typesafe.ai/api, read 2026-09-23). A Score answer's `legend` and
 * `probabilities` are maps keyed by the level index as a string, and every
 * answer carries a `type`. An earlier decoder expected arrays, so every real
 * Jev Score answer would have been quarantined as a schema violation while
 * the stub-backed tests stayed green.
 */
describe("decodeResponse() on TypeSafe's documented response bodies", () => {
  const documented = {
    frustration: score({
      instructions: "How frustrated is the customer?",
      criteria: ["Calm", "Frustrated", "Very angry"],
    }),
    department: choice({
      instructions: "Which team should handle this?",
      criteria: {
        billing: "Payments, invoicing, refunds",
        technical: "Bugs, outages, integrations",
        sales: "Pricing, upgrades, new accounts",
      },
    }),
    is_urgent: noul({ instructions: "Does this convey urgency?" }),
  };
  const body = {
    model: "jev-1.13.0",
    answers: {
      frustration: {
        type: "score",
        score: 1.05,
        legend: { "0": "Calm", "1": "Frustrated", "2": "Very angry" },
        probabilities: { "0": 0.0, "1": 0.95, "2": 0.05 },
        confidence: 0.92,
      },
      department: {
        type: "choice",
        choice: "billing",
        probabilities: { billing: 0.88, technical: 0.12, sales: 0.0 },
        confidence: 0.81,
      },
      is_urgent: { type: "noul", noul: 0.95 },
    },
    usage: { input_tokens: 304, output_tokens: 18 },
  };

  test("a documented Score answer decodes into level order", () => {
    const r = decodeResponse(documented, body);
    assert.equal(r.ok, true, r.ok ? "" : r.error.message);
    if (!r.ok) return;
    assert.deepEqual(r.value.answers.frustration, {
      kind: "score",
      score: 1.05,
      legend: ["Calm", "Frustrated", "Very angry"],
      probabilities: [0.0, 0.95, 0.05],
      confidence: 0.92,
    });
    assert.equal(r.value.model, "jev-1.13.0");
  });

  test("documented Choice and Noul answers decode with their type tags", () => {
    const r = decodeResponse(documented, body);
    assert.equal(r.ok, true, r.ok ? "" : r.error.message);
    if (!r.ok) return;
    assert.equal(r.value.answers.department?.kind, "choice");
    assert.deepEqual(r.value.answers.is_urgent, { kind: "noul", noul: 0.95 });
  });

  test("a Score distribution that skips a level is a schema violation", () => {
    const r = decodeResponse(documented, {
      ...body,
      answers: {
        ...body.answers,
        frustration: {
          ...body.answers.frustration,
          probabilities: { "0": 0.0, "1": 0.95, "3": 0.05 },
        },
      },
    });
    assert.equal(r.ok, false);
    if (r.ok) return;
    assert.equal(r.error.kind, "schema_violation");
    assert.match(r.error.message, /frustration/);
  });

  test("a Score legend that does not cover the scale is a schema violation", () => {
    const r = decodeResponse(documented, {
      ...body,
      answers: {
        ...body.answers,
        frustration: {
          ...body.answers.frustration,
          legend: { "0": "Calm", "1": "Frustrated" },
        },
      },
    });
    assert.equal(r.ok, false);
  });

  test("an answer tagged with a different type than its question is rejected", () => {
    const r = decodeResponse(documented, {
      ...body,
      answers: { ...body.answers, is_urgent: { type: "choice", noul: 0.95 } },
    });
    assert.equal(r.ok, false);
    if (r.ok) return;
    assert.match(r.error.message, /is_urgent/);
  });
});

describe("decodeResponse() on a well-formed body", () => {
  test("returns ok with every answer decoded and tagged by primitive kind", () => {
    const r = decodeResponse(questions, goodBody);
    assert.equal(r.ok, true);
    if (!r.ok) return;

    assert.deepEqual(r.value.answers.team, {
      kind: "choice",
      choice: "billing",
      probabilities: { billing: 0.85, technical: 0.15 },
      confidence: 0.7,
    });
    assert.equal(r.value.answers.urgency?.kind, "score");
    assert.deepEqual(r.value.answers.refund, { kind: "noul", noul: 0.92 });
  });

  test("carries model and token usage through for observability", () => {
    const r = decodeResponse(questions, goodBody);
    assert.equal(r.ok, true);
    if (!r.ok) return;
    assert.equal(r.value.model, "jev-latest");
    assert.deepEqual(r.value.usage, { inputTokens: 412, outputTokens: 33 });
  });
});

describe("decodeResponse() never throws on hostile input", () => {
  const hostile: Array<[string, unknown]> = [
    ["null", null],
    ["undefined", undefined],
    ["a bare string", "not json at all"],
    ["a number", 42],
    ["an array", [1, 2, 3]],
    ["an object with no answers key", { model: "jev-latest" }],
    ["answers as a string", { model: "x", answers: "nope", usage: {} }],
    ["an answers array", { model: "x", answers: [{ noul: 0.5 }] }],
  ];

  for (const [label, body] of hostile) {
    test(`returns a typed failure for ${label}`, () => {
      const r = decodeResponse(questions, body);
      assert.equal(r.ok, false, `${label} should not decode`);
      if (r.ok) return;
      assert.equal(r.error.kind, "schema_violation");
      assert.ok(r.error.message.length > 0);
    });
  }
});

describe("decodeResponse() resists prototype pollution", () => {
  // Two distinct attack shapes. An object literal's `__proto__:` invokes the
  // prototype setter, so `answers` ends up with a hostile prototype and no own
  // keys. `JSON.parse`, which is what an adapter actually produces, does NOT
  // invoke the setter — it creates a real own property named `__proto__`.
  // Both must yield zero answers and leave Object.prototype untouched.
  const attacks: Array<[string, unknown]> = [
    [
      "a hostile prototype via object literal",
      { model: "x", answers: { __proto__: { refund: { noul: 1 } } } },
    ],
    [
      "an own __proto__ key via JSON.parse",
      JSON.parse('{"model":"x","answers":{"__proto__":{"refund":{"noul":1}}}}'),
    ],
  ];

  for (const [label, body] of attacks) {
    test(`treats ${label} as zero answers, not as an inherited answer`, () => {
      const r = decodeResponse(questions, body);

      assert.equal(
        r.ok,
        true,
        "a weird prototype is not itself a malformed body",
      );
      if (!r.ok) return;
      assert.deepEqual(Object.keys(r.value.answers), []);
      assert.deepEqual([...r.value.missing].sort(), [
        "refund",
        "team",
        "urgency",
      ]);
      assert.equal(
        ({} as Record<string, unknown>).refund,
        undefined,
        "Object.prototype must be clean",
      );
    });
  }
});

describe("decodeResponse() enforces the question contract, not just JSON shape", () => {
  test("rejects a choice outside the options we actually asked about", () => {
    const body = structuredClone(goodBody);
    (body.answers.team as { choice: string }).choice = "legal";
    const r = decodeResponse(questions, body);
    assert.equal(r.ok, false);
    if (r.ok) return;
    assert.match(r.error.message, /legal/);
  });

  test("rejects a probability distribution over keys we did not offer", () => {
    const body = structuredClone(goodBody);
    (
      body.answers.team as { probabilities: Record<string, number> }
    ).probabilities = {
      billing: 0.85,
      legal: 0.15,
    };
    assert.equal(decodeResponse(questions, body).ok, false);
  });

  test("rejects a score distribution whose length does not match the scale", () => {
    const body = structuredClone(goodBody);
    (
      body.answers.urgency as { probabilities: Record<string, number> }
    ).probabilities = { "0": 0.5, "1": 0.5 };
    const r = decodeResponse(questions, body);
    assert.equal(r.ok, false);
    if (r.ok) return;
    assert.match(r.error.message, /3/);
  });

  test("rejects a probability outside [0, 1]", () => {
    const body = structuredClone(goodBody);
    (body.answers.refund as { noul: number }).noul = 1.4;
    assert.equal(decodeResponse(questions, body).ok, false);
  });

  test("reports a missing answer rather than discarding the answers that did arrive", () => {
    const body = structuredClone(goodBody);
    delete (body.answers as Record<string, unknown>).refund;
    const r = decodeResponse(questions, body);

    // Whether a gap matters is policy, and policy lives at the gate.
    assert.equal(r.ok, true);
    if (!r.ok) return;
    assert.deepEqual(r.value.missing, ["refund"]);
    assert.equal(r.value.answers.team?.kind, "choice");
    assert.equal(r.value.answers.refund, undefined);
  });

  test("still hard-fails a malformed answer, because a wrong answer beats an absent one", () => {
    const body = structuredClone(goodBody);
    (body.answers as Record<string, unknown>).refund = { noul: "very yes" };
    assert.equal(decodeResponse(questions, body).ok, false);
  });

  test("reports no gaps when every question was answered", () => {
    const r = decodeResponse(questions, goodBody);
    assert.equal(r.ok, true);
    if (!r.ok) return;
    assert.deepEqual(r.value.missing, []);
  });

  test("ignores an answer to a question we never asked rather than failing", () => {
    const body = structuredClone(goodBody);
    (body.answers as Record<string, unknown>).sentiment = { noul: 0.3 };
    const r = decodeResponse(questions, body);
    assert.equal(r.ok, true);
    if (!r.ok) return;
    assert.equal("sentiment" in r.value.answers, false);
  });
});
