import assert from "node:assert/strict";
import { describe, test } from "node:test";

import {
  confidenceOf,
  noulConfidence,
} from "../../src/core/system1/confidence.ts";

describe("noulConfidence()", () => {
  test("is 1 at a certain yes", () => {
    assert.equal(noulConfidence(1), 1);
  });

  test("is 1 at a certain no, because certainty of falsehood is still certainty", () => {
    assert.equal(noulConfidence(0), 1);
  });

  test("is 0 at a coin flip", () => {
    assert.equal(noulConfidence(0.5), 0);
  });

  test("is symmetric around 0.5", () => {
    assert.equal(noulConfidence(0.9), noulConfidence(0.1));
  });

  test("scales linearly with distance from the coin flip", () => {
    assert.ok(Math.abs(noulConfidence(0.75) - 0.5) < 1e-12);
  });
});

describe("confidenceOf()", () => {
  test("passes through the model-reported confidence for a choice", () => {
    assert.equal(
      confidenceOf({
        kind: "choice",
        choice: "billing",
        probabilities: { billing: 0.85, technical: 0.15 },
        confidence: 0.7,
      }),
      0.7,
    );
  });

  test("passes through the model-reported confidence for a score", () => {
    assert.equal(
      confidenceOf({
        kind: "score",
        score: 1.5,
        legend: ["low", "mid", "high"],
        probabilities: [0.2, 0.6, 0.2],
        confidence: 0.6,
      }),
      0.6,
    );
  });

  test("derives confidence for a noul, which Jev does not report one for", () => {
    assert.equal(
      confidenceOf({ kind: "noul", noul: 0.92 }),
      noulConfidence(0.92),
    );
  });
});
