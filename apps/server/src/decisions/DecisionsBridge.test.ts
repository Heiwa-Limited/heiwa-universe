import { assert, it } from "@effect/vitest";
import * as Effect from "effect/Effect";
import { FetchHttpClient, HttpClient, HttpClientResponse } from "effect/http";

import { make } from "./DecisionsBridge.ts";

const ENGINE = "http://engine.test";

const surface = {
  version: "1.0",
  createSurface: {
    surfaceId: "ask-abcd1234",
    catalogId: "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json",
    components: [
      { id: "root", component: "Card", child: "title" },
      { id: "title", component: "Text", text: "Overdue: Pay rent", variant: "body" },
    ],
  },
};

const fakeEngine = (handler: (url: string, body: unknown) => Response) =>
  HttpClient.make((request) =>
    Effect.sync(() => {
      const raw =
        request.body._tag === "Uint8Array" ? new TextDecoder().decode(request.body.body) : null;
      return HttpClientResponse.fromWeb(
        request,
        handler(request.url, raw ? JSON.parse(raw) : null),
      );
    }),
  );

it.effect("reads open decisions from the engine", () =>
  Effect.gen(function* () {
    const http = fakeEngine((url) => {
      assert.strictEqual(url, `${ENGINE}/api/asks`);
      return new Response(
        JSON.stringify({
          open: [{ id: "abcd1234", kind: "reminder_due", title: "Overdue: Pay rent", surface }],
          pendingApproval: 2,
        }),
      );
    });
    const bridge = yield* make(ENGINE).pipe(Effect.provideService(HttpClient.HttpClient, http));
    const status = yield* bridge.status;
    assert.isTrue(status.available);
    assert.strictEqual(status.open[0]?.id, "abcd1234");
    assert.strictEqual(status.pendingApproval, 2);
    assert.isNull(status.detail);
  }),
);

it.effect("reports an unavailable engine instead of failing", () =>
  Effect.gen(function* () {
    // A real refused connection: nothing listens on the discard port.
    const bridge = yield* make("http://127.0.0.1:9").pipe(Effect.provide(FetchHttpClient.layer));
    const status = yield* bridge.status;
    assert.isFalse(status.available);
    assert.deepEqual(status.open, []);
    assert.match(status.detail ?? "", /not running/);
  }),
);

it.effect("rejects a malformed engine payload as unavailable", () =>
  Effect.gen(function* () {
    const http = fakeEngine(() => new Response(JSON.stringify({ open: [{ id: "nope" }] })));
    const bridge = yield* make(ENGINE).pipe(Effect.provideService(HttpClient.HttpClient, http));
    assert.isFalse((yield* bridge.status).available);
  }),
);

it.effect("forwards an answer as an A2UI action with its signature", () =>
  Effect.gen(function* () {
    let sent: unknown = null;
    const http = fakeEngine((url, body) => {
      assert.strictEqual(url, `${ENGINE}/api/action`);
      sent = body;
      return new Response(JSON.stringify({ ok: true, detail: "Overdue: Pay rent" }));
    });
    const bridge = yield* make(ENGINE).pipe(Effect.provideService(HttpClient.HttpClient, http));
    const result = yield* bridge.answer({
      askId: "abcd1234",
      choice: "done",
      sig: "0123456789abcdef01234567",
    });
    assert.isTrue(result.ok);
    assert.deepEqual(sent, {
      action: {
        name: "answer",
        surfaceId: "ask-abcd1234",
        context: { askId: "abcd1234", choice: "done", sig: "0123456789abcdef01234567" },
      },
    });
  }),
);

it.effect("reports a refused answer without failing", () =>
  Effect.gen(function* () {
    const http = fakeEngine(
      () =>
        new Response(JSON.stringify({ ok: false, detail: "That link isn't valid." }), {
          status: 403,
        }),
    );
    const bridge = yield* make(ENGINE).pipe(Effect.provideService(HttpClient.HttpClient, http));
    const result = yield* bridge.answer({
      askId: "abcd1234",
      choice: "done",
      sig: "0123456789abcdef01234567",
    });
    assert.isFalse(result.ok);
    assert.strictEqual(result.detail, "That link isn't valid.");
  }),
);
