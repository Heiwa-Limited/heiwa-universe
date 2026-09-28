import assert from "node:assert/strict";
import { describe, test } from "node:test";
import type { DecodedAnswer } from "../../src/core/system1/types.ts";
import {
  ambiguityMargin,
  DEFAULT_THRESHOLDS,
  gateAnswer,
  gateBatch,
  noulThresholds,
} from "../../src/orchestrator/gate.ts";

function choiceAnswer(
  confidence: number,
  probabilities?: Record<string, number>,
): DecodedAnswer {
  return {
    kind: "choice",
    choice: "billing",
    probabilities: probabilities ?? { billing: 0.9, technical: 0.1 },
    confidence,
  };
}

describe("gateAnswer() confidence bands", () => {
  test("dispatches automatically above the auto threshold", () => {
    assert.equal(gateAnswer(choiceAnswer(0.9)).band, "auto");
  });

  test("deliberates at the auto threshold exactly, because 0.85 is not > 0.85", () => {
    assert.equal(gateAnswer(choiceAnswer(0.85)).band, "deliberate");
  });

  test("deliberates in the middle band", () => {
    assert.equal(gateAnswer(choiceAnswer(0.6)).band, "deliberate");
  });

  test("deliberates at the lower bound of the middle band", () => {
    assert.equal(gateAnswer(choiceAnswer(0.5)).band, "deliberate");
  });

  test("quarantines below the deliberate threshold", () => {
    assert.equal(gateAnswer(choiceAnswer(0.49)).band, "quarantine");
  });

  test("uses the documented default thresholds", () => {
    assert.deepEqual(DEFAULT_THRESHOLDS, { auto: 0.85, deliberate: 0.5 });
  });

  test("honours a per-question threshold override for sensitive judgments", () => {
    const strict = { auto: 0.99, deliberate: 0.9 };
    assert.equal(
      gateAnswer(choiceAnswer(0.95), { thresholds: strict }).band,
      "deliberate",
    );
  });

  test("reports the confidence it gated on and where that number came from", () => {
    const decision = gateAnswer({ kind: "noul", noul: 0.92 });
    assert.equal(decision.confidenceSource, "derived");
    assert.ok(Math.abs(decision.confidence - 0.84) < 1e-9);
    assert.equal(decision.band, "deliberate");
  });
});

describe("ambiguityMargin()", () => {
  test("is the gap between the top two options for a choice", () => {
    assert.ok(
      Math.abs(
        ambiguityMargin(choiceAnswer(0.9, { billing: 0.6, technical: 0.4 })) -
          0.2,
      ) < 1e-9,
    );
  });

  test("is the gap between the top two levels for a score", () => {
    const answer: DecodedAnswer = {
      kind: "score",
      score: 1,
      legend: ["a", "b", "c"],
      probabilities: [0.1, 0.5, 0.4],
      confidence: 0.9,
    };
    assert.ok(Math.abs(ambiguityMargin(answer) - 0.1) < 1e-9);
  });

  test("is the distance from a coin flip for a noul", () => {
    assert.ok(
      Math.abs(ambiguityMargin({ kind: "noul", noul: 0.7 }) - 0.4) < 1e-9,
    );
  });
});

describe("gateAnswer() ambiguity override", () => {
  test("quarantines a confident-looking answer whose top two options are close", () => {
    const answer = choiceAnswer(0.95, {
      billing: 0.5,
      technical: 0.48,
      legal: 0.02,
    });
    const decision = gateAnswer(answer, { minMargin: 0.15 });

    assert.equal(decision.band, "quarantine");
    assert.equal(decision.reason, "ambiguous");
  });

  test("leaves a clear winner alone at the same margin setting", () => {
    const decision = gateAnswer(
      choiceAnswer(0.95, { billing: 0.9, technical: 0.1 }),
      {
        minMargin: 0.15,
      },
    );
    assert.equal(decision.band, "auto");
    assert.equal(decision.reason, "confident");
  });

  test("does not apply a margin check unless one is configured", () => {
    assert.equal(
      gateAnswer(choiceAnswer(0.95, { billing: 0.5, technical: 0.48 })).band,
      "auto",
    );
  });
});

describe("noulThresholds()", () => {
  // Writing `{auto: 0.9}` for a noul silently means "p must be within 0.05 of
  // certain", which is not what it looks like beside a choice's 0.9. This
  // helper takes the probability distance an operator actually has in mind.
  test("converts a probability distance into the equivalent confidence bar", () => {
    const t = noulThresholds({ autoWithin: 0.05, deliberateWithin: 0.25 });
    assert.ok(Math.abs(t.auto - 0.9) < 1e-6);
    assert.ok(Math.abs(t.deliberate - 0.5) < 1e-6);
  });

  test("sits a hair below the nominal bar so the boundary case passes a strict >", () => {
    const t = noulThresholds({ autoWithin: 0.05, deliberateWithin: 0.25 });
    assert.ok(t.auto < 0.9, "must be below 0.9 for |2*0.95-1| to clear it");
  });

  test("admits an answer exactly at the stated distance, unlike a raw > comparison", () => {
    const thresholds = noulThresholds({
      autoWithin: 0.05,
      deliberateWithin: 0.25,
    });
    // p = 0.95 is exactly 0.05 from certain; the operator asked for that to pass.
    assert.equal(
      gateAnswer({ kind: "noul", noul: 0.95 }, { thresholds }).band,
      "auto",
    );
    assert.equal(
      gateAnswer({ kind: "noul", noul: 0.05 }, { thresholds }).band,
      "auto",
    );
  });

  test("rejects an answer just outside the stated distance", () => {
    const thresholds = noulThresholds({
      autoWithin: 0.05,
      deliberateWithin: 0.25,
    });
    assert.equal(
      gateAnswer({ kind: "noul", noul: 0.93 }, { thresholds }).band,
      "deliberate",
    );
  });

  test("quarantines beyond the deliberate distance", () => {
    const thresholds = noulThresholds({
      autoWithin: 0.05,
      deliberateWithin: 0.25,
    });
    assert.equal(
      gateAnswer({ kind: "noul", noul: 0.7 }, { thresholds }).band,
      "quarantine",
    );
  });

  test("rejects a deliberate distance tighter than the auto distance", () => {
    assert.throws(
      () => noulThresholds({ autoWithin: 0.3, deliberateWithin: 0.1 }),
      /deliberateWithin must be at least autoWithin/i,
    );
  });
});

describe("gateAnswer() content veto", () => {
  // The confidence bands measure how SURE the model is, never whether the
  // answer is acceptable. A confident "yes, this is a prompt injection"
  // scores 0.94 and would otherwise sail toward dispatch. Certainty about a
  // dangerous answer is the most dangerous case, not the safest.
  test("quarantines a confidently dangerous answer that the bands would have passed", () => {
    const answer: DecodedAnswer = { kind: "noul", noul: 0.97 };
    assert.equal(gateAnswer(answer).band, "auto");

    const vetoed = gateAnswer(answer, {
      veto: (a) => a.kind === "noul" && a.noul > 0.5,
    });
    assert.equal(vetoed.band, "quarantine");
    assert.equal(vetoed.reason, "vetoed");
  });

  test("leaves an answer alone when the veto predicate does not fire", () => {
    const decision = gateAnswer(
      { kind: "noul", noul: 0.02 },
      { veto: (a) => a.kind === "noul" && a.noul > 0.5 },
    );
    assert.equal(decision.band, "auto");
    assert.equal(decision.reason, "confident");
  });

  test("vetoes even a low-confidence answer, since a maybe-injection is still not dispatchable", () => {
    const decision = gateAnswer(
      { kind: "noul", noul: 0.6 },
      { veto: (a) => a.kind === "noul" && a.noul > 0.5 },
    );
    assert.equal(decision.band, "quarantine");
    assert.equal(decision.reason, "vetoed");
  });

  test("vetoes a choice by its selected option, not by its confidence", () => {
    const answer: DecodedAnswer = {
      kind: "choice",
      choice: "delete_everything",
      probabilities: { delete_everything: 0.99, archive: 0.01 },
      confidence: 0.99,
    };
    const decision = gateAnswer(answer, {
      veto: (a) => a.kind === "choice" && a.choice === "delete_everything",
    });
    assert.equal(decision.band, "quarantine");
  });

  test("treats a throwing veto predicate as a veto, because a broken guard is not a pass", () => {
    const decision = gateAnswer(
      { kind: "noul", noul: 0.02 },
      {
        veto: () => {
          throw new Error("policy bug");
        },
      },
    );
    assert.equal(decision.band, "quarantine");
    assert.equal(decision.reason, "vetoed");
  });
});

describe("gateBatch()", () => {
  const answers: Record<string, DecodedAnswer> = {
    team: choiceAnswer(0.95),
    urgency: {
      kind: "score",
      score: 2,
      legend: ["a", "b", "c"],
      probabilities: [0.05, 0.05, 0.9],
      confidence: 0.9,
    },
    pii: { kind: "noul", noul: 0.55 },
  };

  test("returns a decision for every answer", () => {
    const result = gateBatch(answers);
    assert.deepEqual(Object.keys(result.decisions).sort(), [
      "pii",
      "team",
      "urgency",
    ]);
  });

  test("takes the most conservative band across the batch as the overall verdict", () => {
    // pii at 0.55 derives a confidence of 0.1 -> quarantine, so the batch does.
    assert.equal(gateBatch(answers).band, "quarantine");
  });

  test("names which questions forced the overall verdict", () => {
    assert.deepEqual(gateBatch(answers).blockers, ["pii"]);
  });

  test("ignores questions marked informational when computing the verdict", () => {
    const result = gateBatch(answers, {
      policies: { pii: { informational: true } },
    });
    assert.equal(result.band, "auto");
    assert.deepEqual(result.blockers, []);
    // The decision is still reported, it just does not gate.
    assert.equal(result.decisions.pii?.band, "quarantine");
  });

  test("applies per-question thresholds from policy", () => {
    const result = gateBatch(answers, {
      policies: { team: { thresholds: { auto: 0.99, deliberate: 0.98 } } },
    });
    assert.equal(result.decisions.team?.band, "quarantine");
  });

  test("lets a single vetoed question quarantine the whole batch and name itself", () => {
    const clean: Record<string, DecodedAnswer> = {
      team: choiceAnswer(0.95),
      injection: { kind: "noul", noul: 0.97 },
    };
    const result = gateBatch(clean, {
      policies: {
        injection: { veto: (a) => a.kind === "noul" && a.noul > 0.5 },
      },
    });

    assert.equal(result.band, "quarantine");
    assert.deepEqual(result.blockers, ["injection"]);
    assert.equal(result.decisions.injection?.reason, "vetoed");
  });

  test("reports auto only when every gating question is auto", () => {
    const allGood: Record<string, DecodedAnswer> = {
      team: choiceAnswer(0.95),
      pii: { kind: "noul", noul: 0.99 },
    };
    assert.equal(gateBatch(allGood).band, "auto");
  });
});
