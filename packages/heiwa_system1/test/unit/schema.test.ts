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
      legend: ["low", "mid", "high"],
      probabilities: [0.2, 0.6, 0.2],
      confidence: 0.6,
    },
    refund: { noul: 0.92 },
  },
  usage: { input_tokens: 412, output_tokens: 33 },
};

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
    (body.answers.urgency as { probabilities: number[] }).probabilities = [
      0.5, 0.5,
    ];
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
