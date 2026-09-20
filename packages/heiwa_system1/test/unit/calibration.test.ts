import assert from "node:assert/strict";
import { describe, test } from "node:test";
import type { DecodedAnswer } from "../../src/core/system1/types.ts";
import {
  isAnswerCorrect,
  type LabelledSample,
  metricsAt,
  recommendThresholds,
  sweep,
} from "../../src/orchestrator/calibration.ts";

function ch(choice: string, confidence: number): DecodedAnswer {
  const other = choice === "billing" ? "technical" : "billing";
  return {
    kind: "choice",
    choice,
    probabilities: { [choice]: confidence, [other]: 1 - confidence },
    confidence,
  };
}

/** n samples: `correct` of them right, all at the same confidence. */
function cohort(
  prefix: string,
  n: number,
  correct: number,
  confidence: number,
): LabelledSample[] {
  return Array.from({ length: n }, (_, i) => ({
    id: `${prefix}-${i}`,
    answers: { route: ch(i < correct ? "billing" : "technical", confidence) },
    truth: { route: "billing" },
  }));
}

describe("isAnswerCorrect()", () => {
  test("compares a choice by its selected key", () => {
    assert.equal(isAnswerCorrect(ch("billing", 0.9), "billing"), true);
    assert.equal(isAnswerCorrect(ch("technical", 0.9), "billing"), false);
  });

  test("compares a score within a tolerance, since it is a weighted float", () => {
    const answer: DecodedAnswer = {
      kind: "score",
      score: 1.8,
      legend: ["a", "b", "c"],
      probabilities: [0.1, 0.1, 0.8],
      confidence: 0.9,
    };
    // Ground truth "2" with a 0.5 tolerance: 1.8 rounds to the right level.
    assert.equal(isAnswerCorrect(answer, 2), true);
    assert.equal(isAnswerCorrect(answer, 0), false);
  });

  test("compares a noul by which side of 0.5 it falls", () => {
    assert.equal(isAnswerCorrect({ kind: "noul", noul: 0.92 }, true), true);
    assert.equal(isAnswerCorrect({ kind: "noul", noul: 0.92 }, false), false);
    assert.equal(isAnswerCorrect({ kind: "noul", noul: 0.08 }, false), true);
  });
});

describe("metricsAt()", () => {
  // 10 samples: 8 confident and right, 2 unconfident and wrong.
  const samples = [...cohort("hi", 8, 8, 0.95), ...cohort("lo", 2, 0, 0.3)];

  test("auto-dispatches everything when the bar is on the floor", () => {
    const m = metricsAt(samples, "route", { auto: 0, deliberate: 0 });
    assert.equal(m.autoRate, 1);
    assert.equal(m.quarantineRate, 0);
  });

  test("counts the wrong answers that were auto-dispatched — the dangerous number", () => {
    const m = metricsAt(samples, "route", { auto: 0, deliberate: 0 });
    assert.equal(m.falseAutoCount, 2, "both wrong answers slipped through");
    assert.ok(Math.abs(m.falseAutoRate - 0.2) < 1e-9);
  });

  test("auto-dispatches nothing when the bar is at the ceiling", () => {
    const m = metricsAt(samples, "route", { auto: 1, deliberate: 1 });
    assert.equal(m.autoRate, 0);
    assert.equal(
      m.falseAutoCount,
      0,
      "nothing auto-dispatched means nothing wrongly dispatched",
    );
  });

  test("separates the two unconfident wrong answers out at a sane bar", () => {
    const m = metricsAt(samples, "route", { auto: 0.85, deliberate: 0.5 });
    assert.ok(
      Math.abs(m.autoRate - 0.8) < 1e-9,
      "the 8 confident ones dispatch",
    );
    assert.equal(m.falseAutoCount, 0, "the wrong ones were caught");
    assert.ok(Math.abs(m.quarantineRate - 0.2) < 1e-9);
  });

  test("reports how accurate the auto-dispatched slice actually was", () => {
    const m = metricsAt(samples, "route", { auto: 0.85, deliberate: 0.5 });
    assert.equal(m.autoPrecision, 1, "every fast-path decision was right");
  });

  test("leaves precision undefined rather than claiming 1 when nothing dispatched", () => {
    const m = metricsAt(samples, "route", { auto: 1, deliberate: 1 });
    assert.equal(m.autoPrecision, undefined);
  });

  test("ignores samples with no answer for the question", () => {
    const withGap: LabelledSample[] = [
      ...cohort("hi", 4, 4, 0.95),
      { id: "gap", answers: {}, truth: { route: "billing" } },
    ];
    const m = metricsAt(withGap, "route", { auto: 0.85, deliberate: 0.5 });
    assert.equal(m.evaluated, 4);
  });
});

describe("sweep()", () => {
  const samples = [...cohort("hi", 8, 8, 0.95), ...cohort("lo", 2, 0, 0.3)];

  test("produces a point per candidate threshold", () => {
    const points = sweep(samples, "route", { steps: 11 });
    assert.equal(points.length, 11);
    assert.equal(points[0]?.thresholds.auto, 0);
    assert.equal(points[10]?.thresholds.auto, 1);
  });

  test("auto-dispatch rate never increases as the bar rises", () => {
    const points = sweep(samples, "route", { steps: 21 });
    for (let i = 1; i < points.length; i++) {
      const prev = points[i - 1]?.metrics.autoRate ?? 0;
      const cur = points[i]?.metrics.autoRate ?? 0;
      assert.ok(
        cur <= prev + 1e-9,
        `autoRate rose from ${prev} to ${cur} at a higher bar`,
      );
    }
  });
});

describe("recommendThresholds()", () => {
  const samples = [...cohort("hi", 8, 8, 0.95), ...cohort("lo", 2, 0, 0.3)];

  test("picks the most permissive bar that still meets the safety budget", () => {
    // Maximise throughput subject to no wrong answer being auto-dispatched.
    const rec = recommendThresholds(samples, "route", { maxFalseAutoRate: 0 });
    assert.ok(rec, "a safe operating point exists for this corpus");
    if (!rec) return;
    assert.equal(rec.metrics.falseAutoCount, 0);
    assert.ok(
      rec.metrics.autoRate > 0.7,
      `only ${rec.metrics.autoRate} took the fast path`,
    );
  });

  test("returns nothing when no bar can meet the budget, rather than guessing", () => {
    // Every sample is confidently wrong: no threshold separates them.
    const hopeless = cohort("bad", 10, 0, 0.99);
    const rec = recommendThresholds(hopeless, "route", {
      maxFalseAutoRate: 0,
      minAutoRate: 0.5,
    });
    assert.equal(rec, undefined);
  });

  test("honours a non-zero error budget by dispatching more", () => {
    const mixed = [...cohort("hi", 8, 8, 0.95), ...cohort("mid", 2, 0, 0.9)];
    const strict = recommendThresholds(mixed, "route", { maxFalseAutoRate: 0 });
    const loose = recommendThresholds(mixed, "route", {
      maxFalseAutoRate: 0.25,
    });
    assert.ok(loose, "a 25% error budget should admit an operating point");
    if (!loose || !strict) return;
    assert.ok(
      loose.metrics.autoRate >= strict.metrics.autoRate,
      "a looser budget must not dispatch less",
    );
  });

  test("refuses an empty corpus instead of returning a confident answer about nothing", () => {
    assert.equal(
      recommendThresholds([], "route", { maxFalseAutoRate: 0 }),
      undefined,
    );
  });
});
