import assert from "node:assert/strict";
import { describe, test } from "node:test";

import { choice, noul, score } from "../../src/core/system1/primitives.ts";

describe("choice()", () => {
  test("emits the Jev wire shape with a criteria map", () => {
    const q = choice({
      instructions: "Which team should handle this ticket?",
      criteria: {
        billing: "Payment, invoice, or refund problems",
        technical: "Bugs, outages, or integration failures",
      },
    });

    assert.deepEqual(q.wire, {
      type: "choice",
      instructions: "Which team should handle this ticket?",
      criteria: {
        billing: "Payment, invoice, or refund problems",
        technical: "Bugs, outages, or integration failures",
      },
    });
  });

  test("rejects fewer than two options because one option is not a decision", () => {
    assert.throws(
      () =>
        choice({ instructions: "pick", criteria: { only: "the only one" } }),
      /at least 2 options/i,
    );
  });

  test("rejects more than 255 options, the Jev ceiling", () => {
    const criteria: Record<string, string> = {};
    for (let i = 0; i < 256; i++) criteria[`opt_${i}`] = `option ${i}`;
    assert.throws(
      () => choice({ instructions: "pick", criteria }),
      /at most 255 options/i,
    );
  });
});

describe("score()", () => {
  test("emits the Jev wire shape with an ordered criteria array", () => {
    const q = score({
      instructions: "How urgent is this?",
      criteria: ["Not urgent", "Somewhat urgent", "Critical"],
    });

    assert.deepEqual(q.wire, {
      type: "score",
      instructions: "How urgent is this?",
      criteria: ["Not urgent", "Somewhat urgent", "Critical"],
    });
  });

  test("rejects fewer than two levels because a scale needs a span", () => {
    assert.throws(
      () => score({ instructions: "rate", criteria: ["only level"] }),
      /at least 2 levels/i,
    );
  });
});

describe("noul()", () => {
  test("emits the Jev wire shape and omits criteria when not supplied", () => {
    const q = noul({ instructions: "Does this message request a refund?" });

    assert.deepEqual(q.wire, {
      type: "noul",
      instructions: "Does this message request a refund?",
    });
  });

  // TypeSafe names a Noul's outcomes `true` and `false` on the wire and in
  // its SDK (docs.typesafe.ai/api). An earlier builder sent `yes`/`no`,
  // which no live request had exercised.
  test("carries the optional true/false clarification in the documented keys", () => {
    const q = noul({
      instructions: "Does this contain PII?",
      criteria: {
        true: "Names, emails, card numbers",
        false: "No identifying data",
      },
    });

    assert.deepEqual(q.wire, {
      type: "noul",
      instructions: "Does this contain PII?",
      criteria: {
        true: "Names, emails, card numbers",
        false: "No identifying data",
      },
    });
  });
});

describe("score()", () => {
  test("rejects more than the 10 levels the API accepts", () => {
    const eleven = Array.from({ length: 11 }, (_, i) => `level ${i}`);
    assert.throws(
      () => score({ instructions: "How much?", criteria: eleven }),
      /at most 10 levels/,
    );
  });

  test("accepts exactly 10 levels", () => {
    const ten = Array.from({ length: 10 }, (_, i) => `level ${i}`);
    assert.equal(
      score({ instructions: "How much?", criteria: ten }).levels,
      10,
    );
  });
});

describe("builders", () => {
  test("tag themselves with their primitive kind so answers can be decoded", () => {
    assert.equal(
      choice({ instructions: "x", criteria: { a: "a", b: "b" } }).kind,
      "choice",
    );
    assert.equal(
      score({ instructions: "x", criteria: ["a", "b"] }).kind,
      "score",
    );
    assert.equal(noul({ instructions: "x" }).kind, "noul");
  });

  test("reject blank instructions, which would make the answer meaningless", () => {
    assert.throws(
      () => noul({ instructions: "   " }),
      /instructions must not be empty/i,
    );
  });
});
