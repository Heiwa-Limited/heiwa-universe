import { assert, it } from "@effect/vitest";
import * as Effect from "effect/Effect";
import * as Exit from "effect/Exit";
import * as Layer from "effect/Layer";
import { HttpClient, HttpClientResponse } from "effect/http";

import * as ServerSettings from "../serverSettings.ts";
import { make } from "./LifeSignals.ts";

const FEEDER = "http://feeder.test";
const loops = {
  view: "loops",
  asOf: 1_800_000_000,
  bodies: false,
  messages: [{ who: "Sam", ageHours: 5, triage: "meeting" }],
  mail: [{ who: "Pat", subject: "Lease renewal", ageHours: 72, flagged: false }],
};

const feeder = (handler: (url: string) => Response, seen: Array<string> = []) =>
  HttpClient.make((request) =>
    Effect.sync(() => {
      seen.push(request.url);
      return HttpClientResponse.fromWeb(request, handler(request.url));
    }),
  );

const bridge = (enabled: boolean, http: HttpClient.HttpClient) =>
  make(FEEDER).pipe(
    Effect.provideService(HttpClient.HttpClient, http),
    Effect.provide(Layer.fresh(ServerSettings.layerTest({ lifeSignalsEnabled: enabled }))),
  );

const failureCode = (exit: Exit.Exit<unknown, { readonly code: string }>) =>
  Exit.isFailure(exit) ? exit.cause.reasons.find((r) => r._tag === "Fail")?.error.code : undefined;

it.effect(
  "refuses while the owner has not turned life signals on, without calling the feeder",
  () =>
    Effect.gen(function* () {
      const seen: Array<string> = [];
      const life = yield* bridge(
        false,
        feeder(() => new Response(JSON.stringify(loops)), seen),
      );
      const exit = yield* Effect.exit(life.read({ view: "loops" }));
      assert.strictEqual(failureCode(exit), "permission");
      assert.deepEqual(seen, []);
    }),
);

it.effect("reads a view and passes the date through", () =>
  Effect.gen(function* () {
    const seen: Array<string> = [];
    const life = yield* bridge(
      true,
      feeder(() => new Response(JSON.stringify(loops)), seen),
    );
    const result = yield* life.read({ view: "loops", date: "2026-11-27" });
    assert.strictEqual(result.view, "loops");
    assert.strictEqual(seen[0], `${FEEDER}/api/life?view=loops&date=2026-11-27`);
  }),
);

it.effect("rejects a reply that does not promise metadata only", () =>
  Effect.gen(function* () {
    const withBodies = { ...loops, bodies: true };
    const life = yield* bridge(
      true,
      feeder(() => new Response(JSON.stringify(withBodies))),
    );
    assert.strictEqual(
      failureCode(yield* Effect.exit(life.read({ view: "loops" }))),
      "unavailable",
    );
  }),
);

it.effect("reports a missing feeder as unavailable", () =>
  Effect.gen(function* () {
    const life = yield* bridge(
      true,
      feeder(() => new Response("no", { status: 502 })),
    );
    assert.strictEqual(failureCode(yield* Effect.exit(life.read({ view: "day" }))), "unavailable");
  }),
);
