/**
 * The cross-language gate contract.
 *
 * The gate exists twice: here in TypeScript (calibration and model-doctor
 * tooling, which run outside the runtime) and in Rust (`crates/heiwa_judgment`,
 * which the runtime uses, because provider credentials live in Rust and must
 * never reach the frontend).
 *
 * Two gates that can disagree about whether an action is safe to auto-dispatch
 * is a serious bug, not a style issue. So both read the SAME fixture and must
 * return identical verdicts. Rust is authoritative: it is the one gating real
 * actions. If they disagree, this side is wrong.
 */
import { test, describe } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { gateAnswer, gateBatch, type QuestionPolicy } from "../../src/orchestrator/gate.ts";
import { DEFAULT_THRESHOLDS } from "../../src/orchestrator/gate.ts";
import { confidenceOf } from "../../src/core/system1/confidence.ts";
import type { DecodedAnswer } from "../../src/core/system1/types.ts";

const FIXTURE = fileURLToPath(
  new URL("../../../../crates/heiwa_judgment/testdata/gate_cases.json", import.meta.url),
);

type RawPolicy = {
  thresholds?: { auto: number; deliberate: number };
  min_margin?: number;
  informational?: boolean;
  /** Expressed as data so the fixture stays language-neutral. */
  veto_noul_above?: number;
};

type Cases = {
  defaults: { auto: number; deliberate: number };
  single: Array<{ name: string; answer: DecodedAnswer; policy?: RawPolicy; expect: Record<string, unknown> }>;
  batch: Array<{
    name: string;
    answers: Record<string, DecodedAnswer>;
    policies?: Record<string, RawPolicy>;
    expect: { band: string; blockers: string[] };
  }>;
};

const cases: Cases = JSON.parse(readFileSync(FIXTURE, "utf8"));

/** A predicate cannot live in JSON, so the fixture names it as a threshold. */
function toPolicy(raw: RawPolicy | undefined): QuestionPolicy {
  if (!raw) return {};
  const policy: QuestionPolicy = {};
  if (raw.thresholds) policy.thresholds = raw.thresholds;
  if (raw.min_margin !== undefined) Object.assign(policy, { minMargin: raw.min_margin });
  if (raw.informational) Object.assign(policy, { informational: true });
  if (raw.veto_noul_above !== undefined) {
    const bar = raw.veto_noul_above;
    Object.assign(policy, { veto: (a: DecodedAnswer) => a.kind === "noul" && a.noul > bar });
  }
  return policy;
}

describe("gate contract: defaults", () => {
  test("the shipped thresholds match the fixture", () => {
    assert.deepEqual(DEFAULT_THRESHOLDS, cases.defaults);
  });
});

describe("gate contract: single answers", () => {
  for (const c of cases.single) {
    test(c.name, () => {
      const decision = gateAnswer(c.answer, toPolicy(c.policy));
      assert.equal(decision.band, c.expect.band, "band");
      if (c.expect.reason !== undefined) assert.equal(decision.reason, c.expect.reason, "reason");
      if (c.expect.confidence !== undefined) {
        assert.ok(
          Math.abs(decision.confidence - (c.expect.confidence as number)) < 1e-9,
          `confidence ${decision.confidence} != ${c.expect.confidence}`,
        );
      }
      if (c.expect.confidence_source !== undefined) {
        assert.equal(decision.confidenceSource, c.expect.confidence_source, "confidence source");
      }
      // Whatever the verdict, the confidence gated on must be the one the
      // shared confidence rule produces.
      assert.equal(decision.confidence, confidenceOf(c.answer));
    });
  }
});

describe("gate contract: batches", () => {
  for (const c of cases.batch) {
    test(c.name, () => {
      const policies: Record<string, QuestionPolicy> = {};
      for (const [id, raw] of Object.entries(c.policies ?? {})) policies[id] = toPolicy(raw);

      const result = gateBatch(c.answers, { policies });
      assert.equal(result.band, c.expect.band, "band");
      assert.deepEqual([...result.blockers].sort(), [...c.expect.blockers].sort(), "blockers");
    });
  }
});
