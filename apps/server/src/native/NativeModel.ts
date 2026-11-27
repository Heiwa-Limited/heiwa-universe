import {
  NativeTriageKind,
  NativeWorkflowError,
  type NativeTriageInput,
  type NativeTriageResult,
} from "@t3tools/contracts";
import * as HostProcess from "@t3tools/shared/HostProcess";
import * as Context from "effect/Context";
import * as Effect from "effect/Effect";
import * as FileSystem from "effect/FileSystem";
import * as Layer from "effect/Layer";
import * as Option from "effect/Option";
import * as Path from "effect/Path";
import * as Schema from "effect/Schema";
import * as Semaphore from "effect/Semaphore";
import * as ProcessRunner from "../processRunner.ts";

export const TRIAGE_TIMEOUT_SECONDS = 20;
const Reply = Schema.Struct({
  kind: Schema.optional(NativeTriageKind),
  error: Schema.optional(Schema.String),
  reason: Schema.optional(Schema.String),
});
const encodeJson = Schema.encodeSync(Schema.fromJsonString(Schema.Unknown));
const decodeReply = Schema.decodeUnknownEffect(Schema.fromJsonString(Reply));
const unavailable: Record<string, string> = {
  device: "This Mac does not support Apple Intelligence.",
  disabled: "Turn on Apple Intelligence in System Settings to use on-device triage.",
  "not-ready": "Apple's on-device model is not ready yet. It may still be downloading.",
  os: "On-device triage needs macOS 26 or later.",
  sdk: "This build of Heiwa was compiled without Apple's on-device model.",
};
const failed: Record<string, string> = {
  invalid: "Enter a short excerpt to triage.",
  refused: "Apple's on-device model declined to classify this excerpt.",
  language: "Apple's on-device model does not support this excerpt's language.",
  busy: "The on-device model is busy. Try again in a moment.",
};
const error = (code: NativeWorkflowError["code"], detail: string) =>
  new NativeWorkflowError({ code, detail });

/** Apple's on-device model, run in its own helper process. Nothing is stored or logged. */
export class NativeModel extends Context.Service<
  NativeModel,
  {
    readonly triage: (
      input: NativeTriageInput,
    ) => Effect.Effect<NativeTriageResult, NativeWorkflowError>;
  }
>()("t3/native/NativeModel") {}

export const make = Effect.gen(function* () {
  const platform = yield* HostProcess.Platform;
  const architecture = yield* HostProcess.Architecture;
  const env = yield* HostProcess.Environment;
  const path = yield* Path.Path;
  const fs = yield* FileSystem.FileSystem;
  const runner = yield* ProcessRunner.ProcessRunner;
  const helper =
    env.HEIWA_MODEL_HELPER ??
    path
      .join(import.meta.dirname, "native", "heiwa-model")
      .replace(/app\.asar([/\\])/, "app.asar.unpacked$1");
  // One model launch at a time per server; a second request is refused, never queued.
  const gate = yield* Semaphore.make(1);
  const launch = Effect.fn("NativeModel.launch")(function* (input: NativeTriageInput) {
    if (platform !== "darwin")
      return yield* error("unavailable", "On-device triage runs on a Mac.");
    if (!(yield* fs.exists(helper).pipe(Effect.orElseSucceed(() => false))))
      return yield* error("unavailable", "The on-device model helper is missing from this build.");
    const result = yield* runner
      .run({
        command: helper,
        args: [],
        stdin: encodeJson({ task: "meeting-triage", excerpt: input.excerpt }),
        timeout: `${TRIAGE_TIMEOUT_SECONDS} seconds`,
        maxOutputBytes: 4096,
      })
      .pipe(
        Effect.mapError((cause) =>
          cause._tag === "ProcessTimeoutError"
            ? error(
                "failed",
                `The on-device model did not answer within ${TRIAGE_TIMEOUT_SECONDS} seconds.`,
              )
            : // The helper is arm64 only. Under Rosetta it still launches, so only a failed spawn proves Intel.
              cause._tag === "ProcessSpawnError" && architecture !== "arm64"
              ? error("unavailable", "On-device triage needs a Mac with Apple silicon.")
              : error("failed", "The on-device model helper did not return a result."),
        ),
      );
    const reply = yield* decodeReply(result.stdout).pipe(
      Effect.mapError(() => error("failed", "The on-device model returned an incomplete result.")),
    );
    if (reply.kind !== undefined && reply.error === undefined && result.code === 0)
      return { kind: reply.kind };
    if (reply.error === "unavailable")
      return yield* error(
        "unavailable",
        unavailable[reply.reason ?? ""] ?? "Apple's on-device model is unavailable on this Mac.",
      );
    return yield* error(
      reply.error === "invalid" ? "invalid" : "failed",
      failed[reply.error ?? ""] ?? "The on-device model could not classify this excerpt.",
    );
  });
  return NativeModel.of({
    triage: Effect.fn("NativeModel.triage")(function* (input) {
      const ran = yield* launch(input).pipe(Semaphore.withPermitsIfAvailable(gate, 1));
      if (Option.isNone(ran)) return yield* error("failed", failed.busy!);
      return ran.value;
    }),
  });
});
export const layer = Layer.effect(NativeModel, make).pipe(Layer.provide(ProcessRunner.layer));
