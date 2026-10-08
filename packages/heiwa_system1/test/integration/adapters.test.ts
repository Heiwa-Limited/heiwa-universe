/**
 * These exercise real sockets against a real `node:http` server: real JSON
 * serialisation, real status codes, real aborts. Nothing here is mocked, so a
 * change that breaks the wire contract fails here rather than in production.
 */

import assert from "node:assert/strict";
import http from "node:http";
import type { AddressInfo } from "node:net";
import { after, before, describe, test } from "node:test";
import { failover } from "../../src/core/system1/adapters/failover.ts";
import {
  openRouterAdapter,
  typesafeAdapter,
} from "../../src/core/system1/adapters/http.ts";
import type {
  Adapter,
  AdapterRequest,
} from "../../src/core/system1/adapters/types.ts";
import { err, ok, system1Error } from "../../src/core/system1/errors.ts";

type Handler = (
  req: http.IncomingMessage,
  res: http.ServerResponse,
  body: string,
) => void;

let server: http.Server;
let baseUrl: string;
let handler: Handler;
const received: Array<{
  url: string;
  headers: http.IncomingHttpHeaders;
  body: unknown;
}> = [];

before(async () => {
  server = http.createServer((req, res) => {
    let raw = "";
    req.on("data", (c) => (raw += c));
    req.on("end", () => {
      received.push({
        url: req.url ?? "",
        headers: req.headers,
        body: raw ? JSON.parse(raw) : null,
      });
      handler(req, res, raw);
    });
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  baseUrl = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
});

after(async () => {
  await new Promise<void>((resolve) => server.close(() => resolve()));
});

const answersBody = {
  model: "jev-latest",
  answers: {
    team: { choice: "billing", probabilities: { billing: 1 }, confidence: 0.9 },
  },
  usage: { input_tokens: 12, output_tokens: 2 },
};

function jsonOk(body: unknown): Handler {
  return (_req, res) => {
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify(body));
  };
}

function status(code: number, body = "{}"): Handler {
  return (_req, res) => {
    res.writeHead(code, { "content-type": "application/json" });
    res.end(body);
  };
}

const request: AdapterRequest = {
  body: {
    state: "I was charged twice",
    model: "jev-latest",
    questions: { team: { type: "noul", instructions: "Refund?" } },
  },
};

function call(adapter: Adapter, signal = new AbortController().signal) {
  return adapter.evaluate(request, signal);
}

describe("typesafeAdapter over real HTTP", () => {
  test("posts the System One body to /v1/systemone with bearer auth", async () => {
    handler = jsonOk(answersBody);
    received.length = 0;

    const r = await call(typesafeAdapter({ apiKey: "sk-test-123", baseUrl }));

    assert.equal(r.ok, true);
    assert.equal(received[0]?.url, "/v1/systemone");
    assert.equal(received[0]?.headers.authorization, "Bearer sk-test-123");
    assert.equal(received[0]?.headers["content-type"], "application/json");
    assert.deepEqual(received[0]?.body, request.body);
  });

  test("returns the parsed body untouched for the decoder to validate", async () => {
    handler = jsonOk(answersBody);
    const r = await call(typesafeAdapter({ apiKey: "k", baseUrl }));
    assert.equal(r.ok, true);
    if (!r.ok) return;
    assert.deepEqual(r.value, answersBody);
  });

  test("honours a custom evaluate path", async () => {
    handler = jsonOk(answersBody);
    received.length = 0;
    await call(typesafeAdapter({ apiKey: "k", baseUrl, path: "/v2/decide" }));
    assert.equal(received[0]?.url, "/v2/decide");
  });

  test("refuses to call at all without an api key", async () => {
    const r = await call(typesafeAdapter({ apiKey: "", baseUrl }));
    assert.equal(r.ok, false);
    if (r.ok) return;
    assert.equal(r.error.kind, "not_configured");
  });
});

describe("typesafeAdapter maps documented error statuses", () => {
  const cases: Array<[number, string, boolean]> = [
    [401, "unauthorized", false],
    [422, "invalid_request", false],
    [429, "rate_limited", true],
    [529, "provider_unavailable", true],
    [503, "provider_unavailable", true],
  ];

  for (const [code, kind, retryable] of cases) {
    test(`HTTP ${code} becomes ${kind} (retryable=${retryable})`, async () => {
      handler = status(code);
      const r = await call(typesafeAdapter({ apiKey: "k", baseUrl }));

      assert.equal(r.ok, false);
      if (r.ok) return;
      assert.equal(r.error.kind, kind);
      assert.equal(r.error.retryable, retryable);
      assert.equal(r.error.status, code);
      assert.equal(r.error.adapter, "typesafe");
    });
  }

  test("a 200 that is not JSON becomes a schema violation, not a crash", async () => {
    handler = (_req, res) => {
      res.writeHead(200, { "content-type": "text/html" });
      res.end("<html>hello</html>");
    };
    const r = await call(typesafeAdapter({ apiKey: "k", baseUrl }));

    assert.equal(r.ok, false);
    if (r.ok) return;
    assert.equal(r.error.kind, "schema_violation");
  });

  test("an unreachable host becomes a retryable transport failure", async () => {
    const r = await call(
      typesafeAdapter({ apiKey: "k", baseUrl: "http://127.0.0.1:1" }),
    );
    assert.equal(r.ok, false);
    if (r.ok) return;
    assert.equal(r.error.kind, "transport");
    assert.equal(r.error.retryable, true);
  });
});

describe("typesafeAdapter abort handling", () => {
  test("reports a timeout when the caller's budget signal fires mid-flight", async () => {
    handler = (_req, res) => {
      setTimeout(() => res.end(JSON.stringify(answersBody)), 3000).unref();
    };
    const controller = new AbortController();
    setTimeout(() => controller.abort(), 25).unref();

    const started = Date.now();
    const r = await call(
      typesafeAdapter({ apiKey: "k", baseUrl }),
      controller.signal,
    );

    assert.equal(r.ok, false);
    if (r.ok) return;
    assert.equal(r.error.kind, "timeout");
    assert.ok(
      Date.now() - started < 2000,
      "must abandon the socket, not wait it out",
    );
  });
});

describe("openRouterAdapter", () => {
  test("posts the same System One body to the decisions path with the routed model id", async () => {
    handler = jsonOk(answersBody);
    received.length = 0;

    const r = await call(openRouterAdapter({ apiKey: "or-key", baseUrl }));

    assert.equal(r.ok, true);
    assert.equal(received[0]?.url, "/api/alpha/decisions");
    assert.equal(received[0]?.headers.authorization, "Bearer or-key");
    assert.equal(
      (received[0]?.body as { model: string }).model,
      "typesafe/jev-latest",
    );
  });

  test("keeps state and questions byte-identical to the native call", async () => {
    handler = jsonOk(answersBody);
    received.length = 0;
    await call(openRouterAdapter({ apiKey: "k", baseUrl }));

    assert.deepEqual(
      (received[0]?.body as { state: unknown }).state,
      request.body.state,
    );
    assert.deepEqual(
      (received[0]?.body as { questions: unknown }).questions,
      request.body.questions,
    );
  });

  test("is named distinctly so telemetry can attribute the call", async () => {
    assert.equal(
      openRouterAdapter({ apiKey: "k", baseUrl }).name,
      "openrouter",
    );
  });
});

describe("failover()", () => {
  function stub(
    name: string,
    result: Awaited<ReturnType<Adapter["evaluate"]>>,
  ): Adapter & { calls: number } {
    const a = {
      name,
      calls: 0,
      async evaluate() {
        a.calls++;
        return result;
      },
    };
    return a;
  }

  test("returns the primary's answer without touching the fallback", async () => {
    const primary = stub("primary", ok(answersBody));
    const backup = stub("backup", ok(answersBody));

    const r = await call(failover([primary, backup]));

    assert.equal(r.ok, true);
    assert.equal(primary.calls, 1);
    assert.equal(backup.calls, 0);
  });

  test("falls through to the next adapter when the primary fails", async () => {
    const primary = stub(
      "primary",
      err(system1Error("provider_unavailable", "down")),
    );
    const backup = stub("backup", ok(answersBody));

    const r = await call(failover([primary, backup]));

    assert.equal(r.ok, true);
    assert.equal(backup.calls, 1);
  });

  test("reports the last failure when every adapter is exhausted", async () => {
    const r = await call(
      failover([
        stub("a", err(system1Error("transport", "a died"))),
        stub("b", err(system1Error("rate_limited", "b throttled"))),
      ]),
    );

    assert.equal(r.ok, false);
    if (r.ok) return;
    assert.equal(r.error.kind, "rate_limited");
    assert.match(r.error.message, /b throttled/);
  });

  test("does not burn the fallback on a failure the policy says is not worth retrying", async () => {
    const primary = stub(
      "primary",
      err(system1Error("invalid_request", "bad question")),
    );
    const backup = stub("backup", ok(answersBody));

    const r = await call(
      failover([primary, backup], {
        shouldFailover: (e) => e.kind !== "invalid_request",
      }),
    );

    assert.equal(r.ok, false);
    assert.equal(
      backup.calls,
      0,
      "a malformed request will be malformed everywhere",
    );
  });

  test("refuses an empty adapter list rather than silently succeeding", async () => {
    const r = await call(failover([]));
    assert.equal(r.ok, false);
    if (r.ok) return;
    assert.equal(r.error.kind, "not_configured");
  });
});
