/**
 * Preflight: is this model usable as a System 1 fallback?
 *
 * Every finding this probe reports is one that cost real debugging time
 * during development. The point is that no operator should have to rediscover
 * them by watching a six-question fan fail for 75 seconds.
 */

import assert from "node:assert/strict";
import http from "node:http";
import type { AddressInfo } from "node:net";
import { after, before, describe, test } from "node:test";

import {
  type ProbeFinding,
  probeModel,
} from "../../src/core/system1/adapters/probe.ts";

let server: http.Server;
let baseUrl: string;
let reply: (body: unknown) => unknown;
let delayMs = 0;

before(async () => {
  server = http.createServer((req, res) => {
    let raw = "";
    req.on("data", (c) => (raw += c));
    req.on("end", () => {
      const send = () => {
        res.writeHead(200, { "content-type": "application/json" });
        res.end(JSON.stringify(reply(raw ? JSON.parse(raw) : null)));
      };
      if (delayMs > 0) setTimeout(send, delayMs).unref();
      else send();
    });
  });
  await new Promise<void>((r) => server.listen(0, "127.0.0.1", r));
  baseUrl = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
});

after(async () => {
  delayMs = 0;
  await new Promise<void>((r) => server.close(() => r()));
});

function completion(content: unknown, extra: Record<string, unknown> = {}) {
  return {
    choices: [
      {
        finish_reason: "stop",
        message: { role: "assistant", content: JSON.stringify(content) },
        ...extra,
      },
    ],
    usage: { prompt_tokens: 80, completion_tokens: 20 },
  };
}

/** A model that answers correctly and in range. */
function goodAnswer() {
  return {
    probe_choice: { choice: "yes" },
    probe_noul: { noul: 0.9 },
    probe_score: { score: 1 },
  };
}

function codes(findings: readonly ProbeFinding[]): string[] {
  return findings.map((f) => f.code).sort();
}

describe("probeModel() on a well-behaved model", () => {
  test("reports it suitable with no blockers", async () => {
    reply = () => completion(goodAnswer());
    const r = await probeModel({ baseUrl, model: "good" });

    assert.equal(r.reachable, true);
    assert.equal(r.suitable, true);
    assert.deepEqual(
      r.findings.filter((f) => f.severity === "blocker"),
      [],
    );
  });

  test("measures latency so an operator can compare against their budget", async () => {
    reply = () => completion(goodAnswer());
    const r = await probeModel({ baseUrl, model: "good" });
    assert.ok(r.latencyMs >= 0);
    assert.equal(typeof r.latencyMs, "number");
  });
});

describe("probeModel() detects a reasoning model", () => {
  test("flags a model that spends its budget thinking and answers nothing", async () => {
    reply = () => ({
      choices: [
        {
          finish_reason: "length",
          message: { content: "", reasoning: "Thinking...".repeat(200) },
        },
      ],
      usage: { prompt_tokens: 80, completion_tokens: 900 },
    });

    const r = await probeModel({ baseUrl, model: "reasoner" });

    assert.equal(r.suitable, false);
    assert.ok(codes(r.findings).includes("reasons_before_answering"));
    const finding = r.findings.find(
      (f) => f.code === "reasons_before_answering",
    );
    assert.equal(finding?.severity, "blocker");
    assert.match(finding?.remedy ?? "", /non-reasoning|maxTokens/i);
  });

  test("flags a model that emits reasoning even when it does answer", async () => {
    reply = () =>
      completion(goodAnswer(), {
        message: {
          content: JSON.stringify(goodAnswer()),
          reasoning: "hmm".repeat(200),
        },
      });
    const r = await probeModel({ baseUrl, model: "chatty" });

    assert.ok(codes(r.findings).includes("emits_reasoning"));
    assert.equal(
      r.findings.find((f) => f.code === "emits_reasoning")?.severity,
      "warning",
    );
    assert.equal(
      r.suitable,
      true,
      "a warning must not make an answering model unusable",
    );
  });
});

describe("probeModel() detects an unconstrained decoder", () => {
  test("flags a model that returns an option we never offered", async () => {
    reply = () =>
      completion({ ...goodAnswer(), probe_choice: { choice: "banana" } });
    const r = await probeModel({ baseUrl, model: "loose" });

    assert.equal(r.suitable, false);
    assert.ok(codes(r.findings).includes("ignores_enum"));
    assert.equal(
      r.findings.find((f) => f.code === "ignores_enum")?.severity,
      "blocker",
    );
  });

  test("flags a model that ignores numeric bounds — measured real behaviour", async () => {
    // Ollama does exactly this: honours enum, ignores minimum/maximum.
    reply = () => completion({ ...goodAnswer(), probe_noul: { noul: 4 } });
    const r = await probeModel({ baseUrl, model: "unbounded" });

    assert.ok(codes(r.findings).includes("ignores_numeric_bounds"));
    const finding = r.findings.find((f) => f.code === "ignores_numeric_bounds");
    // Caught by the decoder downstream, so it degrades rather than blocks.
    assert.equal(finding?.severity, "warning");
    assert.match(finding?.detail ?? "", /4/);
  });

  test("flags a score outside its declared scale", async () => {
    reply = () => completion({ ...goodAnswer(), probe_score: { score: -3 } });
    const r = await probeModel({ baseUrl, model: "negative" });
    assert.ok(codes(r.findings).includes("ignores_numeric_bounds"));
  });
});

describe("probeModel() on an unusable endpoint", () => {
  test("reports unreachable rather than throwing", async () => {
    const r = await probeModel({
      baseUrl: "http://127.0.0.1:1",
      model: "nothing",
    });

    assert.equal(r.reachable, false);
    assert.equal(r.suitable, false);
    assert.ok(codes(r.findings).includes("unreachable"));
  });

  test("flags a model that returns unparseable content", async () => {
    reply = () => ({
      choices: [
        { finish_reason: "stop", message: { content: "sure, it's yes" } },
      ],
    });
    const r = await probeModel({ baseUrl, model: "prose" });

    assert.equal(r.suitable, false);
    assert.ok(codes(r.findings).includes("unparseable"));
  });

  test("respects a timeout instead of hanging on a slow model", async () => {
    delayMs = 3000;
    const started = Date.now();
    const r = await probeModel({ baseUrl, model: "slow", timeoutMs: 100 });
    delayMs = 0;

    assert.equal(r.suitable, false);
    assert.ok(codes(r.findings).includes("timeout"));
    assert.ok(
      Date.now() - started < 2000,
      "must abandon the probe, not wait it out",
    );
  });
});

describe("probeModel() findings are actionable", () => {
  test("every finding carries a non-empty remedy", async () => {
    reply = () => ({
      choices: [
        {
          finish_reason: "length",
          message: { content: "", reasoning: "x".repeat(500) },
        },
      ],
    });
    const r = await probeModel({ baseUrl, model: "bad" });

    assert.ok(r.findings.length > 0);
    for (const f of r.findings) {
      assert.ok(f.remedy.length > 0, `${f.code} has no remedy`);
      assert.ok(f.detail.length > 0, `${f.code} has no detail`);
    }
  });

  test("renders a human summary an operator can paste into an issue", async () => {
    reply = () => completion(goodAnswer());
    const r = await probeModel({ baseUrl, model: "good" });
    assert.match(r.summary, /good/);
    assert.match(r.summary, /suitable/i);
  });
});
