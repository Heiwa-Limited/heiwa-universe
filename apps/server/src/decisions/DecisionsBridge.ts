/**
 * Bridge to this Mac's decisions engine (`~/heiwa-root/asks_server.py`, loopback only).
 * Buttons carry the engine's per-choice HMAC, so answers are forwarded without its key;
 * a missing engine is a normal state, reported as `available: false`, never an error.
 */
import {
  Decision,
  type DecisionAnswerInput,
  DecisionAnswerResult,
  type DecisionsStatus,
} from "@t3tools/contracts";
import * as Context from "effect/Context";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as Schema from "effect/Schema";
import { FetchHttpClient, HttpClient, HttpClientRequest } from "effect/http";

const DEFAULT_ENGINE_URL = "http://127.0.0.1:8790";

const EngineAsks = Schema.Struct({
  open: Schema.Array(Decision),
  pendingApproval: Schema.Number,
});

const EngineAnswer = Schema.Struct({ ok: Schema.Boolean, detail: Schema.String });

const unavailable = (detail: string): DecisionsStatus => ({
  available: false,
  open: [],
  pendingApproval: 0,
  detail,
});

export const make = Effect.fn("DecisionsBridge.make")(function* (
  engineUrl: string = process.env.HEIWA_DECISIONS_URL ?? DEFAULT_ENGINE_URL,
) {
  const http = yield* HttpClient.HttpClient;

  const status: Effect.Effect<DecisionsStatus> = Effect.gen(function* () {
    const response = yield* http.execute(HttpClientRequest.get(`${engineUrl}/api/asks`));
    if (response.status !== 200) {
      return unavailable(`The decisions engine answered ${response.status}.`);
    }
    const body = yield* response.json.pipe(Effect.flatMap(Schema.decodeUnknownEffect(EngineAsks)));
    return {
      available: true,
      open: body.open,
      pendingApproval: body.pendingApproval,
      detail: null,
    };
  }).pipe(
    Effect.timeout("3 seconds"),
    Effect.orElseSucceed(() => unavailable("The decisions engine is not running on this Mac.")),
  );

  const answer = Effect.fn("DecisionsBridge.answer")(
    function* (input: DecisionAnswerInput) {
      const request = yield* HttpClientRequest.post(`${engineUrl}/api/action`).pipe(
        HttpClientRequest.bodyJson({
          action: {
            name: "answer",
            surfaceId: `ask-${input.askId}`,
            context: { askId: input.askId, choice: input.choice, sig: input.sig },
          },
        }),
      );
      const response = yield* http.execute(request);
      const body = yield* response.json.pipe(
        Effect.flatMap(Schema.decodeUnknownEffect(EngineAnswer)),
      );
      const result: DecisionAnswerResult = {
        ok: response.status === 200 && body.ok,
        detail: body.detail,
      };
      return result;
    },
    Effect.timeout("5 seconds"),
    Effect.orElseSucceed((): DecisionAnswerResult => ({
      ok: false,
      detail: "The decisions engine could not record that answer.",
    })),
  );

  return { status, answer };
});

export class DecisionsBridge extends Context.Service<
  DecisionsBridge,
  Effect.Success<ReturnType<typeof make>>
>()("t3/decisions/DecisionsBridge") {}

export const layer = Layer.effect(DecisionsBridge, make()).pipe(
  Layer.provide(FetchHttpClient.layer),
);
