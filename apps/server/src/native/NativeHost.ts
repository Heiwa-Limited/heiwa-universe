// @effect-diagnostics nodeBuiltinImport:off -- SHA-256 binds opaque revisions to complete native snapshots.
import { createHash } from "node:crypto";
import {
  NativeApp,
  NativeRecord,
  NativeResource,
  NativeWorkflowError,
  type NativeAction,
  type NativeReadInput,
} from "@t3tools/contracts";
import * as HostProcess from "@t3tools/shared/HostProcess";
import * as Context from "effect/Context";
import * as Effect from "effect/Effect";
import * as FileSystem from "effect/FileSystem";
import * as Layer from "effect/Layer";
import * as Path from "effect/Path";
import * as Schema from "effect/Schema";
import * as ProcessRunner from "../processRunner.ts";
import { NOTES_SCRIPT } from "./NotesScript.ts";

export const NativeSnapshot = Schema.Struct({
  ...NativeRecord.fields,
  revision: Schema.optional(Schema.String),
  bodyHtml: Schema.optional(Schema.String.check(Schema.isMaxLength(16000))),
  recurring: Schema.Boolean,
  hasAttendees: Schema.Boolean,
  contentTruncated: Schema.optional(Schema.Boolean),
  hasAttachments: Schema.optional(Schema.Boolean),
});
export type NativeSnapshot = typeof NativeSnapshot.Type;
const Reply = Schema.Struct({
  error: Schema.optional(Schema.String),
  resources: Schema.optional(Schema.Array(NativeResource)),
  records: Schema.optional(Schema.Array(NativeSnapshot)),
  truncated: Schema.optional(Schema.Boolean),
  record: Schema.optional(NativeSnapshot),
  deleted: Schema.optional(Schema.Boolean),
});
const encodeJson = Schema.encodeSync(Schema.fromJsonString(Schema.Unknown));
const decodeReply = Schema.decodeUnknownEffect(Schema.fromJsonString(Reply));
export const nativeRecord = (raw: NativeSnapshot): typeof NativeRecord.Type => {
  const { revision: _revision, ...snapshot } = raw;
  const canonical = Object.fromEntries(
    Object.entries(snapshot).sort(([a], [b]) => a.localeCompare(b)),
  );
  const {
    bodyHtml: _html,
    recurring: _recurring,
    hasAttendees: _attendees,
    contentTruncated: _truncated,
    hasAttachments: _attachments,
    ...record
  } = raw;
  return {
    ...record,
    revision: createHash("sha256").update(JSON.stringify(canonical)).digest("hex"),
  };
};

export class NativeHost extends Context.Service<
  NativeHost,
  {
    readonly platform: string;
    readonly inventory: (
      app: NativeApp,
      requestAccess: boolean,
    ) => Effect.Effect<ReadonlyArray<typeof NativeResource.Type>, NativeWorkflowError>;
    readonly read: (
      input: NativeReadInput,
      ids: ReadonlyArray<string>,
    ) => Effect.Effect<
      { readonly records: ReadonlyArray<NativeSnapshot>; readonly truncated: boolean },
      NativeWorkflowError
    >;
    readonly inspect: (
      app: NativeApp,
      resourceId: string,
      id: string,
    ) => Effect.Effect<NativeSnapshot, NativeWorkflowError>;
    readonly apply: (
      action: NativeAction,
      expected: NativeSnapshot | null,
    ) => Effect.Effect<NativeSnapshot | null, NativeWorkflowError>;
  }
>()("t3/native/NativeHost") {}

const make = Effect.gen(function* () {
  const platform = yield* HostProcess.Platform;
  const env = yield* HostProcess.Environment;
  const path = yield* Path.Path;
  const fs = yield* FileSystem.FileSystem;
  const runner = yield* ProcessRunner.ProcessRunner;
  const bundledPath = path.join(import.meta.dirname, "native", "heiwa-apple");
  const helper =
    env.HEIWA_APPLE_HELPER ?? bundledPath.replace(/app\.asar([/\\])/, "app.asar.unpacked$1");
  const invoke = Effect.fn("NativeHost.invoke")(function* (app: NativeApp, request: object) {
    if (platform !== "darwin")
      return yield* new NativeWorkflowError({
        code: "unavailable",
        detail: "Apple workflows run on a Mac.",
      });
    if (app !== "notes" && !(yield* fs.exists(helper).pipe(Effect.orElseSucceed(() => false)))) {
      return yield* new NativeWorkflowError({
        code: "unavailable",
        detail: "The Apple workflow helper is missing from this build.",
      });
    }
    const result = yield* runner
      .run({
        command: app === "notes" ? "/usr/bin/osascript" : helper,
        args: app === "notes" ? ["-l", "JavaScript", "-e", NOTES_SCRIPT] : [],
        stdin: encodeJson({ app, ...request }),
        timeout: "15 seconds",
        maxOutputBytes: 2 * 1024 * 1024,
      })
      .pipe(
        Effect.mapError(
          () =>
            new NativeWorkflowError({
              code: "uncertain",
              detail:
                "The Apple action did not return a complete result. Check the native app before retrying a write.",
            }),
        ),
      );
    const reply = yield* decodeReply(result.stdout).pipe(
      Effect.mapError(
        () =>
          new NativeWorkflowError({
            code: "uncertain",
            detail: "The Apple helper returned an incomplete result.",
          }),
      ),
    );
    if (reply.error !== undefined || result.code !== 0) {
      const code =
        reply.error === "permission"
          ? "permission"
          : reply.error === "stale"
            ? "stale"
            : reply.error === "invalid"
              ? "invalid"
              : "uncertain";
      return yield* new NativeWorkflowError({
        code,
        detail:
          code === "permission"
            ? "Allow Heiwa to access this Apple app in System Settings, then reconnect."
            : code === "stale"
              ? "The item changed. Read it again and create a new proposal."
              : "The native action could not be verified. Inspect the Apple app before retrying.",
      });
    }
    return reply;
  });
  return {
    platform,
    inventory: Effect.fn("NativeHost.inventory")(function* (app, requestAccess) {
      const result = yield* invoke(app, { operation: "inventory", requestAccess });
      if (result.resources === undefined)
        return yield* new NativeWorkflowError({
          code: "failed",
          detail: "No resource inventory returned.",
        });
      return result.resources;
    }),
    read: Effect.fn("NativeHost.read")(function* (input, resourceIds) {
      if (resourceIds.length === 0) return { records: [], truncated: false };
      const result = yield* invoke(input.app, {
        ...input,
        operation: "read",
        resourceIds,
        limit: input.limit ?? 20,
      });
      if (result.records === undefined || result.truncated === undefined)
        return yield* new NativeWorkflowError({
          code: "failed",
          detail: "No complete native read returned.",
        });
      return { records: result.records, truncated: result.truncated };
    }),
    inspect: Effect.fn("NativeHost.inspect")(function* (app, resourceId, id) {
      const result = yield* invoke(app, { operation: "inspect", resourceIds: [resourceId], id });
      if (result.record === undefined)
        return yield* new NativeWorkflowError({
          code: "stale",
          detail: "This item is no longer available.",
        });
      return result.record;
    }),
    apply: Effect.fn("NativeHost.apply")(function* (action, expected) {
      const result = yield* invoke(action.app, {
        ...action,
        ...(action.operation === "create" ? { body: action.body ?? "" } : {}),
        resourceIds: [action.resourceId],
        expected,
      });
      if (action.operation === "trash" && result.deleted === true) return null;
      if (result.record === undefined)
        return yield* new NativeWorkflowError({
          code: "uncertain",
          detail: "The native write has no verified readback.",
        });
      return result.record;
    }),
  } satisfies NativeHost["Service"];
});
export const layer = Layer.effect(NativeHost, make).pipe(Layer.provide(ProcessRunner.layer));
