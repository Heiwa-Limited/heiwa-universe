// @effect-diagnostics nodeBuiltinImport:off -- SHA-256 binds opaque revisions to complete native snapshots.
import * as NodeCrypto from "node:crypto";
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
import * as Schedule from "effect/Schedule";
import * as Schema from "effect/Schema";
import * as ProcessRunner from "../processRunner.ts";
import * as MailIndex from "./MailIndex.ts";
import { MAIL_SCRIPT } from "./MailScript.ts";
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
const MailReply = Schema.Struct({
  error: Schema.optional(Schema.String),
  accounts: Schema.optional(
    Schema.Array(Schema.Struct({ id: Schema.String, name: Schema.String })),
  ),
  contents: Schema.optional(
    Schema.Array(Schema.Struct({ id: Schema.String, content: Schema.String })),
  ),
  completed: Schema.optional(Schema.Boolean),
  deleted: Schema.optional(Schema.Boolean),
  id: Schema.optional(Schema.String),
  subject: Schema.optional(Schema.String),
  content: Schema.optional(Schema.String),
});
const encodeJson = Schema.encodeSync(Schema.fromJsonString(Schema.Unknown));
const decodeReply = Schema.decodeUnknownEffect(Schema.fromJsonString(Reply));
const decodeMailReply = Schema.decodeUnknownEffect(Schema.fromJsonString(MailReply));
export const nativeRecord = (raw: NativeSnapshot): typeof NativeRecord.Type => {
  // Mail content is fetched on demand, so it must not change an item's revision.
  const { revision: _revision, content: _content, ...snapshot } = raw;
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
    revision: NodeCrypto.createHash("sha256").update(JSON.stringify(canonical)).digest("hex"),
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
    return yield* checked(result, decodeReply);
  });
  const checked = Effect.fn("NativeHost.checked")(function* <
    A extends { readonly error?: string | undefined },
  >(
    result: { readonly stdout: string; readonly code: unknown },
    decode: (input: unknown) => Effect.Effect<A, Schema.SchemaError>,
  ) {
    const reply = yield* decode(result.stdout).pipe(
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
          : reply.error === "stale" || reply.error === "missing"
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
  const unavailable = (detail: string) => new NativeWorkflowError({ code: "unavailable", detail });
  const mailScript = Effect.fn("NativeHost.mailScript")(function* (request: object) {
    if (platform !== "darwin") return yield* unavailable("Apple workflows run on a Mac.");
    const result = yield* runner
      .run({
        command: "/usr/bin/osascript",
        args: ["-l", "JavaScript", "-e", MAIL_SCRIPT],
        stdin: encodeJson(request),
        // Saving a draft waits for Mail to file it; large IMAP mailboxes answer slowly.
        timeout: "30 seconds",
        maxOutputBytes: 2 * 1024 * 1024,
      })
      .pipe(
        Effect.mapError(
          () =>
            new NativeWorkflowError({
              code: "uncertain",
              detail: "Mail did not return a complete result. Check Mail before retrying a change.",
            }),
        ),
      );
    return yield* checked(result, decodeMailReply);
  });
  /** Opens the index per call: Mail rewrites it constantly and a held handle pins stale WAL pages. */
  const withMailIndex = Effect.fn("NativeHost.withMailIndex")(function* <A>(
    use: (db: ReturnType<typeof MailIndex.open>) => A,
  ) {
    if (platform !== "darwin") return yield* unavailable("Apple workflows run on a Mac.");
    const root = path.join(env.HOME ?? "", "Library", "Mail");
    const entries = yield* fs.readDirectory(root).pipe(
      Effect.mapError(
        () =>
          new NativeWorkflowError({
            code: "permission",
            detail:
              "Allow Heiwa Full Disk Access in System Settings › Privacy & Security so it can read Mail's index, then reconnect.",
          }),
      ),
    );
    const file = MailIndex.indexPath(root, entries);
    if (file === null) return yield* unavailable("Mail has no local mailboxes on this Mac yet.");
    return yield* Effect.try({
      try: () => {
        const db = MailIndex.open(file);
        try {
          return use(db);
        } finally {
          db.close();
        }
      },
      catch: () =>
        new NativeWorkflowError({
          code: "permission",
          detail:
            "Heiwa could not read Mail's index. Allow Full Disk Access in System Settings, then reconnect.",
        }),
    });
  });
  const mailTarget = (resourceId: string) => {
    const mailbox = MailIndex.mailboxFromUrl(resourceId);
    return mailbox === null
      ? Effect.fail(new NativeWorkflowError({ code: "invalid", detail: "Unknown Mail mailbox." }))
      : Effect.succeed(mailbox);
  };
  /** Mail updates its index asynchronously after a scripted change. */
  const mailReadback = (id: string, settled: (row: NativeSnapshot) => boolean) =>
    withMailIndex((db) => MailIndex.message(db, id)).pipe(
      Effect.filterOrFail(
        (row): row is NativeSnapshot => row !== undefined && settled(row),
        (row) => ({ unsettled: row ?? null }),
      ),
      Effect.retry({ times: 20, schedule: Schedule.spaced("250 millis") }),
      Effect.catchIf(
        (error): error is { unsettled: NativeSnapshot | null } => "unsettled" in error,
        (error) => Effect.succeed(error.unsettled),
      ),
    );
  const mail = {
    inventory: Effect.fn("NativeHost.mail.inventory")(function* () {
      const accounts = (yield* mailScript({ operation: "accounts" })).accounts ?? [];
      const rows = yield* withMailIndex(MailIndex.mailboxes);
      return rows.flatMap((row) => {
        const account = accounts.find((value) => value.id === row.accountId);
        return account === undefined
          ? []
          : [
              {
                app: "mail" as const,
                id: row.url.slice(0, 1024),
                title: row.title.slice(0, 16000),
                source: account.name.slice(0, 16000),
                writable: true,
              },
            ];
      });
    }),
    read: Effect.fn("NativeHost.mail.read")(function* (
      input: NativeReadInput,
      resourceIds: ReadonlyArray<string>,
    ) {
      if (input.ids === undefined)
        return yield* withMailIndex((db) =>
          MailIndex.messages(db, {
            urls: resourceIds,
            limit: input.limit ?? 20,
            includeRead: input.includeCompleted ?? false,
            query: input.query,
            start: input.start,
            end: input.end,
          }),
        );
      const ids = input.ids;
      const rows = (yield* withMailIndex((db) =>
        ids.map((id) => MailIndex.message(db, id)),
      )).filter(
        (row): row is NativeSnapshot => row !== undefined && resourceIds.includes(row.resourceId),
      );
      const contents = new Map<string, string>();
      for (const url of new Set(rows.map((row) => row.resourceId))) {
        const group = rows.filter((row) => row.resourceId === url);
        const target = yield* mailTarget(url);
        const reply = yield* mailScript({
          operation: "content",
          accountId: target.accountId,
          path: target.path,
          ids: group.map((row) => row.id),
        });
        for (const value of reply.contents ?? []) contents.set(value.id, value.content);
      }
      return {
        records: rows.map((row) => ({ ...row, content: contents.get(row.id) ?? "" })),
        truncated: rows.length < ids.length,
      };
    }),
    inspect: Effect.fn("NativeHost.mail.inspect")(function* (resourceId: string, id: string) {
      const row = yield* withMailIndex((db) => MailIndex.message(db, id));
      if (row === undefined || row.resourceId !== resourceId)
        return yield* new NativeWorkflowError({
          code: "stale",
          detail: "This message is no longer in that mailbox.",
        });
      return row;
    }),
    apply: Effect.fn("NativeHost.mail.apply")(function* (action: NativeAction) {
      const target = yield* mailTarget(action.resourceId);
      const base = { accountId: target.accountId, path: target.path };
      if (action.operation === "trash") {
        const reply = yield* mailScript({ ...base, operation: "trash", id: action.id });
        return reply.deleted === true ? null : yield* incomplete;
      }
      if (action.operation === "complete") {
        yield* mailScript({
          ...base,
          operation: "complete",
          id: action.id,
          completed: action.completed,
        });
        return (
          (yield* mailReadback(action.id!, (row) => row.completed === action.completed)) ??
          (yield* incomplete)
        );
      }
      const afterId = yield* withMailIndex(MailIndex.highWater);
      const reply = yield* mailScript({
        ...base,
        operation: "create",
        title: action.title,
        body: action.body ?? "",
        recipients: action.recipients ?? [],
      });
      if (reply.id === undefined) return yield* incomplete;
      const requested = action.recipients?.length ?? 0;
      const indexed = yield* withMailIndex((db) =>
        MailIndex.drafts(db, { accountId: target.accountId, subject: action.title!, afterId }),
      ).pipe(
        Effect.filterOrFail(
          (rows) => rows.length === 1 && (rows[0]!.recipients?.length ?? 0) >= requested,
          () => ({ unsettled: null }),
        ),
        Effect.map((rows) => rows[0]!),
        Effect.retry({ times: 40, schedule: Schedule.spaced("250 millis") }),
        Effect.catchIf(
          (error): error is { unsettled: null } => "unsettled" in error,
          () => Effect.succeed(null),
        ),
      );
      if (indexed === null) return yield* incomplete;
      // The index carries only a summary; Mail's own readback proves the saved text.
      return {
        ...indexed,
        title: reply.subject ?? indexed.title,
        body: MailIndex.normalizeText(reply.content ?? ""),
        recipients: indexed.recipients?.map(MailIndex.address),
      };
    }),
  };
  const incomplete = Effect.fail(
    new NativeWorkflowError({
      code: "uncertain",
      detail: "The Mail change has no verified readback. Check Mail before retrying.",
    }),
  );
  return {
    platform,
    inventory: Effect.fn("NativeHost.inventory")(function* (app, requestAccess) {
      if (app === "mail") return yield* mail.inventory();
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
      if (input.app === "mail") return yield* mail.read(input, resourceIds);
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
      if (app === "mail") return yield* mail.inspect(resourceId, id);
      const result = yield* invoke(app, { operation: "inspect", resourceIds: [resourceId], id });
      if (result.record === undefined)
        return yield* new NativeWorkflowError({
          code: "stale",
          detail: "This item is no longer available.",
        });
      return result.record;
    }),
    apply: Effect.fn("NativeHost.apply")(function* (action, expected) {
      if (action.app === "mail") return yield* mail.apply(action);
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
