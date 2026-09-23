/**
 * A redirect must never carry the state to a second origin.
 *
 * `fetch` follows redirects by default, and a 307/308 re-sends the original
 * POST body to wherever `Location` points — a destination nobody classified.
 * Both adapters refuse to follow; the Rust runtime does the same with a typed
 * `redirected` failure (`crates/heiwa_judgment`, `System1Client`).
 */

import assert from "node:assert/strict";
import http from "node:http";
import type { AddressInfo } from "node:net";
import { after, before, describe, test } from "node:test";

import { typesafeAdapter } from "../../src/core/system1/adapters/http.ts";
import { structuredLlmAdapter } from "../../src/core/system1/adapters/structured_llm.ts";
import type { AdapterRequest } from "../../src/core/system1/adapters/types.ts";

const SENTINEL = "SYNTHETIC_LOCAL_ONLY_SENTINEL";

let origin: http.Server;
let elsewhere: http.Server;
let originUrl: string;
let elsewhereUrl: string;
const reachedElsewhere: string[] = [];

function listen(server: http.Server): Promise<string> {
  return new Promise((resolve) =>
    server.listen(0, "127.0.0.1", () =>
      resolve(`http://127.0.0.1:${(server.address() as AddressInfo).port}`),
    ),
  );
}

before(async () => {
  elsewhere = http.createServer((req, res) => {
    let raw = "";
    req.on("data", (chunk) => {
      raw += chunk;
    });
    req.on("end", () => {
      reachedElsewhere.push(raw);
      res.writeHead(200, { "content-type": "application/json" });
      res.end("{}");
    });
  });
  elsewhereUrl = await listen(elsewhere);
  origin = http.createServer((req, res) => {
    req.resume();
    req.on("end", () => {
      res.writeHead(307, { location: `${elsewhereUrl}${req.url ?? "/"}` });
      res.end();
    });
  });
  originUrl = await listen(origin);
});

after(async () => {
  await new Promise<void>((resolve) => origin.close(() => resolve()));
  await new Promise<void>((resolve) => elsewhere.close(() => resolve()));
});

const request: AdapterRequest = {
  body: {
    state: SENTINEL,
    model: "jev-latest",
    questions: { flag: { type: "noul", instructions: "Is it flagged?" } },
  },
};

describe("adapters never follow a redirect", () => {
  test("the TypeSafe adapter fails instead of re-sending to Location", async () => {
    reachedElsewhere.length = 0;
    const result = await typesafeAdapter({
      apiKey: "k",
      baseUrl: originUrl,
    }).evaluate(request, new AbortController().signal);
    assert.equal(result.ok, false);
    assert.deepEqual(
      reachedElsewhere,
      [],
      "the redirect target received the state",
    );
  });

  test("the structured fallback fails instead of re-sending to Location", async () => {
    reachedElsewhere.length = 0;
    const result = await structuredLlmAdapter({
      baseUrl: originUrl,
      model: "m",
    }).evaluate(request, new AbortController().signal);
    assert.equal(result.ok, false);
    assert.deepEqual(
      reachedElsewhere,
      [],
      "the redirect target received the state",
    );
  });
});
