import assert from "node:assert/strict";
import { describe, test } from "node:test";
import { INBOUND_QUESTIONS } from "../../src/examples/routing_agent/questions.ts";
import {
  slaHoursFor,
  TOOLS,
  toolFor,
} from "../../src/examples/routing_agent/tools.ts";

describe("toolFor()", () => {
  test("returns the tool registered for a known route", () => {
    assert.equal(toolFor("billing"), TOOLS.billing);
    assert.equal(toolFor("technical"), TOOLS.technical);
  });

  test("sends an unrecognised route to manual review, NOT to a default tool", () => {
    // Defaulting an unknown route to `billing` would quietly wire it to the
    // refund service. Failing to a human is the only safe default here.
    const result = toolFor("some_route_nobody_registered")({
      state: {},
      route: "some_route_nobody_registered",
      urgency: 0,
      sentiment: 0,
      containsPii: false,
    });
    assert.equal(result.tool, "manual_queue");
  });

  test("every route the question can return has a registered tool", () => {
    // The guard above is the safety net; this is the assertion that the net
    // should never be needed. Adding a fifth route without a tool fails here.
    for (const option of INBOUND_QUESTIONS.route.options) {
      assert.ok(
        TOOLS[option],
        `route option "${option}" has no registered tool`,
      );
    }
  });
});

describe("slaHoursFor()", () => {
  test("maps the urgency floor to the slowest SLA", () => {
    assert.equal(slaHoursFor(0), 72);
  });

  test("maps the urgency ceiling to the fastest SLA", () => {
    assert.equal(slaHoursFor(3), 1);
  });

  test("interpolates a fractional score rather than truncating it", () => {
    // Score is probability-weighted, so 1.5 is a real value, not an index.
    const mid = slaHoursFor(1.5);
    assert.ok(mid < 24 && mid > 8, `expected between 8 and 24, got ${mid}`);
  });

  test("clamps above the top of the scale instead of reading past the ladder", () => {
    assert.equal(slaHoursFor(9), 1);
  });
});
