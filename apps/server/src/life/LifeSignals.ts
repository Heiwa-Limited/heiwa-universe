/**
 * Life signals from this Mac's feeder (`~/heiwa-root/life.py` via asks_server, loopback only).
 * Metadata only: the feeder never returns message bodies, and this bridge rejects any reply
 * that does not say so. Off unless the owner turns on `lifeSignalsEnabled`.
 */
import { LifeSignalsResult, type LifeSignalsInput, NativeWorkflowError } from "@t3tools/contracts";
import * as Context from "effect/Context";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as Schema from "effect/Schema";
import { FetchHttpClient, HttpClient, HttpClientRequest } from "effect/http";
import * as ServerSettings from "../serverSettings.ts";

const DEFAULT_FEEDER_URL = "http://127.0.0.1:8790";
const decodeResult = Schema.decodeUnknownEffect(LifeSignalsResult);

const off = new NativeWorkflowError({
  code: "permission",
  detail: "Life signals are off on this machine. The user can turn them on in Settings → Labs.",
});
const unreachable = new NativeWorkflowError({
  code: "unavailable",
  detail: "The life signals feeder is not running on this Mac.",
});

export const make = Effect.fn("LifeSignals.make")(function* (
  feederUrl: string = process.env.HEIWA_LIFE_URL ?? DEFAULT_FEEDER_URL,
) {
  const http = yield* HttpClient.HttpClient;
  const settings = yield* ServerSettings.ServerSettingsService;
  const enabled = settings.getSettings.pipe(
    Effect.map((s) => s.lifeSignalsEnabled),
    Effect.orElseSucceed(() => false),
  );

  const fetchView = Effect.fn("LifeSignals.fetchView")(
    function* (url: string) {
      const response = yield* http.execute(HttpClientRequest.get(url));
      if (response.status !== 200) return yield* unreachable;
      const body = yield* response.json;
      return yield* decodeResult(body);
    },
    Effect.timeout("5 seconds"),
    Effect.mapError(() => unreachable),
  );

  const read = Effect.fn("LifeSignals.read")(function* (input: LifeSignalsInput) {
    if (!(yield* enabled)) return yield* off;
    const url = new URL("/api/life", feederUrl);
    url.searchParams.set("view", input.view);
    if (input.date !== undefined) url.searchParams.set("date", input.date);
    return yield* fetchView(url.toString());
  });

  return { read };
});

export class LifeSignals extends Context.Service<
  LifeSignals,
  Effect.Success<ReturnType<typeof make>>
>()("t3/life/LifeSignals") {}

export const layer = Layer.effect(LifeSignals, make()).pipe(Layer.provide(FetchHttpClient.layer));
