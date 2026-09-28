/**
 * Retry and telemetry behaviour of System1Client.
 *
 * Written after the implementation rather than before it — a TDD lapse
 * recorded honestly. These tests exist to find out whether the retry path
 * actually behaves as documented, not to confirm that it does.
 */

import assert from "node:assert/strict";
import { describe, test } from "node:test";
import type { Adapter } from "../../src/core/system1/adapters/types.ts";
import { System1Client } from "../../src/core/system1/client.ts";
import type { System1ErrorKind } from "../../src/core/system1/errors.ts";
import { err, ok, system1Error } from "../../src/core/system1/errors.ts";
import { memorySink } from "../../src/core/system1/observability.ts";
import { noul } from "../../src/core/system1/primitives.ts";

const questions = { refund: noul({ instructions: "Refund requested?" }) };

const goodBody = {
  model: "jev-latest",
  answers: { refund: { noul: 0.9 } },
  usage: { input_tokens: 10, output_tokens: 2 },
};

/** Fails `failures` times with `kind`, then succeeds. */
function flaky(
  failures: number,
  kind: System1ErrorKind = "provider_unavailable",
) {
  const state = { calls: 0 };
  const adapter: Adapter = {
    name: "flaky",
    async evaluate() {
      state.calls++;
      if (state.calls <= failures) {
        return err(
          system1Error(kind, `attempt ${state.calls} failed`, {
            adapter: "flaky",
          }),
        );
      }
      return ok(goodBody);
    },
  };
  return { adapter, state };
}

describe("System1Client retry", () => {
  test("does not retry at all by default", async () => {
    const { adapter, state } = flaky(1);
    const r = await new System1Client({ adapter }).evaluate({
      state: "x",
      questions,
    });

    assert.equal(state.calls, 1);
    assert.equal(r.ok, false);
  });

  test("retries a retryable failure and succeeds", async () => {
    const { adapter, state } = flaky(2);
    const client = new System1Client({
      adapter,
      maxAttempts: 3,
      budgetMs: 2000,
    });

    const r = await client.evaluate({ state: "x", questions });

    assert.equal(r.ok, true, "third attempt should succeed");
    assert.equal(state.calls, 3);
    if (!r.ok) return;
    assert.equal(r.value.attempts, 3);
  });

  test("gives up after maxAttempts and returns the last error", async () => {
    const { adapter, state } = flaky(99, "rate_limited");
    const client = new System1Client({
      adapter,
      maxAttempts: 3,
      budgetMs: 2000,
    });

    const r = await client.evaluate({ state: "x", questions });

    assert.equal(state.calls, 3);
    assert.equal(r.ok, false);
    if (r.ok) return;
    assert.equal(r.error.kind, "rate_limited");
  });

  test("does not retry an unauthorized failure, since no backoff fixes a bad key", async () => {
    const { adapter, state } = flaky(99, "unauthorized");
    const client = new System1Client({
      adapter,
      maxAttempts: 5,
      budgetMs: 2000,
    });

    const r = await client.evaluate({ state: "x", questions });

    assert.equal(state.calls, 1);
    assert.equal(r.ok, false);
  });

  test("does not retry a schema violation, since the same body fails identically", async () => {
    let calls = 0;
    const adapter: Adapter = {
      name: "drifted",
      async evaluate() {
        calls++;
        return ok({
          model: "x",
          answers: { refund: { noul: "not a number" } },
        });
      },
    };
    const client = new System1Client({
      adapter,
      maxAttempts: 5,
      budgetMs: 2000,
    });

    const r = await client.evaluate({ state: "x", questions });

    assert.equal(calls, 1, "a contract violation is not transient");
    assert.equal(r.ok, false);
    if (r.ok) return;
    assert.equal(r.error.kind, "schema_violation");
  });

  test("stops retrying rather than overrunning the latency budget", async () => {
    const { adapter, state } = flaky(99);
    // 30ms budget cannot fit attempt 1 + 25ms backoff + attempt 2.
    const client = new System1Client({
      adapter,
      maxAttempts: 10,
      budgetMs: 30,
    });

    const started = Date.now();
    const r = await client.evaluate({ state: "x", questions });
    const elapsed = Date.now() - started;

    assert.equal(r.ok, false);
    assert.ok(
      state.calls < 10,
      `made ${state.calls} attempts inside a 30ms budget`,
    );
    assert.ok(elapsed < 400, `took ${elapsed}ms against a 30ms budget`);
  });

  test("backs off between attempts rather than hammering immediately", async () => {
    const stamps: number[] = [];
    const adapter: Adapter = {
      name: "stamping",
      async evaluate() {
        stamps.push(Date.now());
        return err(system1Error("transport", "nope", { adapter: "stamping" }));
      },
    };
    await new System1Client({
      adapter,
      maxAttempts: 3,
      budgetMs: 5000,
    }).evaluate({
      state: "x",
      questions,
    });

    assert.equal(stamps.length, 3);
    const first = stamps[0] ?? 0;
    const second = stamps[1] ?? 0;
    const third = stamps[2] ?? 0;
    assert.ok(
      second - first >= 20,
      `first gap was ${second - first}ms, expected ~25ms`,
    );
    assert.ok(
      third - second >= 40,
      `second gap was ${third - second}ms, expected ~50ms`,
    );
  });
});

describe("System1Client telemetry", () => {
  test("emits one record per call with latency, tokens and per-question confidence", async () => {
    const telemetry = memorySink();
    const client = new System1Client({
      adapter: {
        name: "fine",
        async evaluate() {
          return ok(goodBody);
        },
      },
      telemetry,
    });

    await client.evaluate({ state: "x", questions });

    assert.equal(telemetry.records.length, 1);
    const r = telemetry.records[0];
    assert.ok(r);
    assert.equal(r.adapter, "fine");
    assert.equal(r.questionCount, 1);
    assert.equal(r.attempts, 1);
    assert.equal(r.inputTokens, 10);
    assert.deepEqual(r.questions, [
      {
        id: "refund",
        kind: "noul",
        confidence: 0.8,
        confidenceSource: "derived",
      },
    ]);
  });

  test("emits a record on failure too, carrying the error", async () => {
    const telemetry = memorySink();
    const client = new System1Client({
      adapter: {
        name: "down",
        async evaluate() {
          return err(
            system1Error("provider_unavailable", "down", { adapter: "down" }),
          );
        },
      },
      telemetry,
    });

    await client.evaluate({ state: "x", questions });

    const r = telemetry.records[0];
    assert.ok(r);
    assert.equal(r.error?.kind, "provider_unavailable");
    assert.equal(r.questions.length, 0);
  });

  test("records the attempt count so a flapping provider is visible", async () => {
    const telemetry = memorySink();
    const { adapter } = flaky(1);
    await new System1Client({
      adapter,
      maxAttempts: 3,
      budgetMs: 2000,
      telemetry,
    }).evaluate({
      state: "x",
      questions,
    });

    assert.equal(telemetry.records[0]?.attempts, 2);
  });

  test("flags a call that returned but blew the budget", async () => {
    const telemetry = memorySink();
    const client = new System1Client({
      adapter: {
        name: "slow",
        async evaluate() {
          await new Promise((r) => setTimeout(r, 40));
          return ok(goodBody);
        },
      },
      budgetMs: 10,
      telemetry,
    });

    await client.evaluate({ state: "x", questions });
    // Either the abort fired (timeout) or it returned late; both must be
    // visible rather than silently passing as a healthy fast-path call.
    const r = telemetry.records[0];
    assert.ok(r);
    assert.ok(
      r.overBudget || r.error?.kind === "timeout",
      "a blown budget must be observable",
    );
  });

  test("a throwing telemetry sink cannot take down the decision path", async () => {
    const client = new System1Client({
      adapter: {
        name: "fine",
        async evaluate() {
          return ok(goodBody);
        },
      },
      telemetry: () => {
        throw new Error("the log shipper is down");
      },
    });

    const r = await client.evaluate({ state: "x", questions });
    assert.equal(r.ok, true);
  });
});
