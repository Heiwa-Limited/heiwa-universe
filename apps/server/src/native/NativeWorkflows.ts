// @effect-diagnostics nodeBuiltinImport:off -- proposal identifiers are generated on the host, never by a model.
import * as NodeCrypto from "node:crypto";
import {
  isNativeInstant,
  NativeAction,
  NativeApp,
  NativeProposal,
  NativeResource,
  NativeSource,
  NativeWriteMode,
  NativeWorkflowError,
  type NativeReadInput,
  type NativeWorkflowInput,
} from "@t3tools/contracts";
import * as Context from "effect/Context";
import * as DateTime from "effect/DateTime";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as PubSub from "effect/PubSub";
import * as Schema from "effect/Schema";
import * as Semaphore from "effect/Semaphore";
import * as Stream from "effect/Stream";
import * as SqlClient from "effect/sql/SqlClient";
import * as MailIndex from "./MailIndex.ts";
import * as NativeHost from "./NativeHost.ts";

const encodeJson = Schema.encodeSync(Schema.fromJsonString(Schema.Unknown));
const decodeResource = Schema.decodeUnknownEffect(Schema.fromJsonString(NativeResource));
const decodeResources = Schema.decodeUnknownEffect(
  Schema.fromJsonString(Schema.Array(NativeResource)),
);
const decodeIds = Schema.decodeUnknownEffect(Schema.fromJsonString(Schema.Array(Schema.String)));
const decodeAction = Schema.decodeUnknownEffect(Schema.fromJsonString(NativeAction));
const decodeSource = Schema.decodeUnknownEffect(Schema.fromJsonString(NativeSource));
const decodeSnapshot = Schema.decodeUnknownEffect(Schema.fromJsonString(NativeHost.NativeSnapshot));
const decodeRecord = Schema.decodeUnknownEffect(
  Schema.fromJsonString(NativeProposal.fields.result),
);
type ProposalRow = {
  id: string;
  action_json: string;
  expected_json: string | null;
  status: (typeof NativeProposal.Type)["status"];
  created_at: string;
  result_json: string | null;
  detail: string | null;
  resource_json: string | null;
  source_json: string | null;
  origin: string;
};
const isNativeWorkflowError = Schema.is(NativeWorkflowError);
const failure = (code: NativeWorkflowError["code"], detail: string) =>
  new NativeWorkflowError({ code, detail });
export const reviewRequired = () =>
  failure(
    "permission",
    "This action needs user review. Use heiwa_native_propose and tell the user approval is needed.",
  );
const storageError = () => failure("failed", "Heiwa could not read or save the workflow state.");
const guarded = <A, E, R>(effect: Effect.Effect<A, E, R>) =>
  effect.pipe(Effect.mapError((error) => (isNativeWorkflowError(error) ? error : storageError())));
const decodeRow = Effect.fn("NativeWorkflows.decodeRow")(function* (row: ProposalRow) {
  return {
    id: row.id,
    action: yield* decodeAction(row.action_json),
    resource: row.resource_json === null ? null : yield* decodeResource(row.resource_json),
    before:
      row.expected_json === null
        ? null
        : NativeHost.nativeRecord(yield* decodeSnapshot(row.expected_json)),
    source: row.source_json === null ? null : yield* decodeSource(row.source_json),
    status: row.status,
    createdAt: row.created_at,
    result: row.result_json === null ? null : yield* decodeRecord(row.result_json),
    detail: row.detail,
  };
});
export const validateAction = (action: typeof NativeAction.Type) => {
  // Mail supports read state, drafts and Trash. Heiwa never sends or edits received mail.
  if (action.app === "mail" && !["create", "complete", "trash"].includes(action.operation))
    return false;
  if (action.recipients !== undefined && (action.app !== "mail" || action.operation !== "create"))
    return false;
  if (
    action.operation === "append" &&
    (action.app !== "notes" || !action.body?.trim() || action.title !== undefined)
  )
    return false;
  if (
    action.operation === "update" &&
    [
      action.title,
      action.body,
      action.start,
      action.end,
      action.due,
      action.priority,
      action.allDay,
    ].every((value) => value === undefined)
  )
    return false;
  if (action.operation === "create") {
    if (
      !action.title?.trim() ||
      action.title.length > 800 ||
      action.id !== undefined ||
      action.expectedRevision !== undefined
    )
      return false;
  } else if (action.id === undefined || action.expectedRevision === undefined) return false;
  if (action.title !== undefined && (!action.title.trim() || action.title.length > 800))
    return false;
  if (
    action.operation === "complete" &&
    ((action.app !== "reminders" && action.app !== "mail") || action.completed === undefined)
  )
    return false;
  if (action.app !== "reminders" && (action.due !== undefined || action.priority !== undefined))
    return false;
  if (action.app !== "calendar" && action.allDay !== undefined) return false;
  if (
    action.due !== undefined &&
    action.due !== null &&
    !isNativeInstant(action.due) &&
    !(/^\d{4}-\d{2}-\d{2}$/.test(action.due) && isNativeInstant(`${action.due}T00:00:00Z`))
  )
    return false;
  if (action.app !== "calendar" && (action.start !== undefined || action.end !== undefined))
    return false;
  if (action.operation !== "complete" && action.completed !== undefined) return false;
  if (
    action.operation === "trash" &&
    [
      action.title,
      action.body,
      action.start,
      action.end,
      action.due,
      action.priority,
      action.allDay,
    ].some((v) => v !== undefined)
  )
    return false;
  if (
    action.app === "calendar" &&
    action.operation === "create" &&
    (action.start === undefined || action.end === undefined)
  )
    return false;
  for (const value of [action.start, action.end])
    if (value !== undefined && !isNativeInstant(value)) return false;
  if (
    action.start !== undefined &&
    action.end !== undefined &&
    Date.parse(action.end) <= Date.parse(action.start)
  )
    return false;
  return true;
};

const mailAccount = (url: string) => /^[a-z]+:\/\/[^/]+\//.exec(url)?.[0];
export const sameMailAccount = (a: string, b: string) =>
  mailAccount(a) !== undefined && mailAccount(a) === mailAccount(b);

export const nativeDayWindow = (now: DateTime.Utc, zone: DateTime.TimeZone) => {
  const start = DateTime.startOf(DateTime.setZone(now, zone), "day");
  const end = DateTime.add(start, { days: 1 });
  return {
    start: DateTime.formatIso(start).replace(".000Z", "Z"),
    end: DateTime.formatIso(end).replace(".000Z", "Z"),
  };
};

export class NativeExecutionAuthority extends Context.Service<
  NativeExecutionAuthority,
  { readonly origin: string; readonly check: Effect.Effect<void, NativeWorkflowError> }
>()("t3/native/NativeWorkflows/NativeExecutionAuthority") {}

const make = Effect.gen(function* () {
  const sql = yield* SqlClient.SqlClient;
  const host = yield* NativeHost.NativeHost;
  const gate = yield* Semaphore.make(1);
  // Subscribers re-read the whole status, so only the newest signal matters.
  const changes = yield* PubSub.sliding<void>(1);
  const announcing = <A, E, R>(effect: Effect.Effect<A, E, R>) =>
    effect.pipe(Effect.ensuring(PubSub.publish(changes, undefined)));
  const grants = Effect.fn("NativeWorkflows.grants")(function* () {
    const rows = yield* sql<{
      app: typeof NativeApp.Type;
      resource_ids_json: string;
      resources_json: string;
      write_mode: typeof NativeWriteMode.Type;
    }>`SELECT app, resource_ids_json, resources_json, write_mode FROM heiwa_native_grants ORDER BY app`;
    return yield* Effect.forEach(rows, (row) =>
      Effect.gen(function* () {
        return {
          app: row.app,
          resourceIds: yield* decodeIds(row.resource_ids_json),
          resources: yield* decodeResources(row.resources_json),
          writeMode: row.write_mode,
        };
      }),
    );
  });
  const selected = Effect.fn("NativeWorkflows.selected")(function* (app: typeof NativeApp.Type) {
    const all = yield* grants();
    return all.find((row) => row.app === app)?.resourceIds ?? [];
  });
  const proposals = Effect.fn("NativeWorkflows.proposals")(function* () {
    const rows =
      yield* sql<ProposalRow>`SELECT * FROM heiwa_native_proposals WHERE status IN ('pending','applying','uncertain') ORDER BY created_at DESC LIMIT 50`;
    return yield* Effect.forEach(rows, decodeRow);
  });
  const status = Effect.fn("NativeWorkflows.status")(function* () {
    const recentRows =
      yield* sql<ProposalRow>`SELECT * FROM heiwa_native_proposals ORDER BY created_at DESC LIMIT 10`;
    return {
      platform: host.platform,
      grants: yield* grants(),
      pending: yield* proposals(),
      recent: yield* Effect.forEach(recentRows, decodeRow),
    };
  });
  const readRaw = Effect.fn("NativeWorkflows.readRaw")(function* (input: NativeReadInput) {
    if (input.app !== "mail" && (input.query !== undefined || input.ids !== undefined))
      return yield* failure("invalid", "Search and reads by id are available for Mail only.");
    if (input.app === "calendar") {
      if (
        input.start === undefined ||
        input.end === undefined ||
        !isNativeInstant(input.start) ||
        !isNativeInstant(input.end) ||
        Date.parse(input.end) <= Date.parse(input.start) ||
        Date.parse(input.end) - Date.parse(input.start) > 31 * 86400000
      ) {
        return yield* failure(
          "invalid",
          "Calendar reads need a valid start and end within 31 days.",
        );
      }
    }
    return yield* host.read(input, yield* selected(input.app));
  });
  const read = Effect.fn("NativeWorkflows.read")(function* (input: NativeReadInput) {
    const result = yield* readRaw(input);
    return {
      records: result.records.map(NativeHost.nativeRecord),
      truncated:
        result.truncated || result.records.some((record) => record.contentTruncated === true),
    };
  });
  const requireSelection = Effect.fn("NativeWorkflows.requireSelection")(function* (
    action: typeof NativeAction.Type,
  ) {
    if (!(yield* selected(action.app)).includes(action.resourceId))
      return yield* failure(
        "selection",
        "Choose this calendar, list, folder or mailbox in Apple apps settings before using it.",
      );
  });
  const findExpected = Effect.fn("NativeWorkflows.findExpected")(function* (
    action: typeof NativeAction.Type,
  ) {
    if (action.operation === "create") return null;
    const raw = yield* host.inspect(action.app, action.resourceId, action.id!);
    if (raw === undefined || NativeHost.nativeRecord(raw).revision !== action.expectedRevision)
      return yield* failure(
        "stale",
        "The item is unavailable or changed. Read it again before proposing a change.",
      );
    if (
      raw.locked ||
      raw.contentTruncated ||
      (["update", "append"].includes(action.operation) && raw.hasAttachments) ||
      raw.recurring ||
      raw.hasAttendees
    )
      return yield* failure(
        "invalid",
        "This item needs its native app: Heiwa does not edit recurring, invited, attached or truncated content.",
      );
    return raw;
  });
  // The stored title and sender come from the item itself, so an agent cannot invent where a change came from.
  const findSource = Effect.fn("NativeWorkflows.findSource")(function* (
    ref: NonNullable<(typeof NativeAction.Type)["source"]>,
  ) {
    if (!(yield* selected(ref.app)).includes(ref.resourceId))
      return yield* failure("selection", "The source item is outside what the user selected.");
    const raw = yield* host
      .inspect(ref.app, ref.resourceId, ref.id)
      .pipe(Effect.orElseSucceed(() => undefined));
    if (raw === undefined)
      return yield* failure("stale", "The source item is unavailable. Read it again.");
    return {
      ...ref,
      title: raw.title,
      ...(raw.sender === undefined ? {} : { sender: raw.sender }),
    } satisfies typeof NativeSource.Type;
  });
  const propose = Effect.fn("NativeWorkflows.propose")(function* (
    { source: sourceRef, ...action }: typeof NativeAction.Type,
    origin = "owner",
  ) {
    if (!validateAction(action))
      return yield* failure(
        "invalid",
        "The proposed Apple action has invalid or incompatible fields.",
      );
    yield* requireSelection(action);
    const resource = (yield* host.inventory(action.app, false)).find(
      (row) => row.id === action.resourceId,
    );
    if (!resource?.writable)
      return yield* failure("selection", "This resource is missing or read only.");
    const expected = yield* findExpected(action);
    const source = sourceRef === undefined ? null : yield* findSource(sourceRef);
    const id = NodeCrypto.randomUUID(),
      createdAt = DateTime.formatIso(yield* DateTime.now);
    yield* sql`INSERT INTO heiwa_native_proposals (id, action_json, expected_json, resource_json, source_json, origin, status, created_at) VALUES (${id}, ${encodeJson(action)}, ${expected === null ? null : encodeJson(expected)}, ${encodeJson(resource)}, ${source === null ? null : encodeJson(source)}, ${origin}, 'pending', ${createdAt})`;
    return {
      id,
      action,
      resource,
      before: expected === null ? null : NativeHost.nativeRecord(expected),
      source,
      status: "pending" as const,
      createdAt,
      result: null,
      detail: null,
    };
  });
  const approve = Effect.fn("NativeWorkflows.approve")(function* (id: string, approve: boolean) {
    const rows = yield* sql<ProposalRow>`SELECT * FROM heiwa_native_proposals WHERE id = ${id}`;
    const row = rows[0];
    if (row === undefined) return yield* failure("invalid", "No workflow proposal has this id.");
    if (row.status === "applied" || row.status === "rejected" || row.status === "acknowledged")
      return yield* decodeRow(row);
    if (row.status !== "pending")
      return yield* failure(
        "uncertain",
        "This proposal already started. Inspect its native result; it will not run again automatically.",
      );
    const action = yield* decodeAction(row.action_json);
    if (!approve) {
      yield* sql`UPDATE heiwa_native_proposals SET status = 'rejected' WHERE id = ${id} AND status = 'pending'`;
      return { ...(yield* decodeRow(row)), status: "rejected" as const };
    }
    yield* requireSelection(action);
    const currentResource = (yield* host.inventory(action.app, false)).find(
      (value) => value.id === action.resourceId,
    );
    const approvedResource =
      row.resource_json === null ? null : yield* decodeResource(row.resource_json);
    if (
      !currentResource?.writable ||
      !approvedResource ||
      currentResource.title !== approvedResource.title ||
      currentResource.source !== approvedResource.source
    )
      return yield* failure(
        "stale",
        "The target calendar, list, folder or mailbox changed. Create a fresh proposal.",
      );
    const expected = row.expected_json === null ? null : yield* decodeSnapshot(row.expected_json);
    // Claim durably before crossing the process boundary. A crash leaves an explicit uncertain operation.
    const claim =
      yield* sql`UPDATE heiwa_native_proposals SET status = 'applying' WHERE id = ${id} AND status = 'pending' RETURNING id`;
    if (claim.length !== 1)
      return yield* failure("uncertain", "Another request already claimed this proposal.");
    const outcome = yield* host.apply(action, expected).pipe(Effect.result);
    if (outcome._tag === "Failure") {
      const error = outcome.failure;
      const status: "uncertain" | "failed" = error.code === "uncertain" ? "uncertain" : "failed";
      yield* sql`UPDATE heiwa_native_proposals SET status = ${status}, detail = ${error.detail} WHERE id = ${id}`;
      return { ...(yield* decodeRow(row)), status, detail: error.detail };
    }
    const raw = outcome.success;
    const verified =
      action.operation === "trash"
        ? raw === null
        : raw !== null &&
          (action.app === "mail" && action.operation === "create"
            ? // Drafts land in the account's own drafts mailbox, not the selected one.
              sameMailAccount(raw.resourceId, action.resourceId)
            : raw.resourceId === action.resourceId) &&
          (action.recipients === undefined ||
            (raw.recipients ?? []).join("\n").toLowerCase() ===
              action.recipients.join("\n").toLowerCase()) &&
          (action.title === undefined || raw.title === action.title) &&
          (action.body === undefined ||
            (action.app === "mail"
              ? MailIndex.normalizeText(raw.body) === MailIndex.normalizeText(action.body)
              : action.operation === "append"
                ? raw.body.replace(/\n$/, "").endsWith(action.body.replace(/\n$/, "")) &&
                  raw.body.startsWith(expected?.body ?? "")
                : raw.body.replace(/\n$/, "") === action.body.replace(/\n$/, ""))) &&
          (action.completed === undefined || raw.completed === action.completed) &&
          (action.due === undefined ||
            raw.due === action.due ||
            (typeof action.due === "string" &&
              action.due.length > 10 &&
              Date.parse(raw.due ?? "") === Date.parse(action.due))) &&
          (action.priority === undefined || raw.priority === action.priority) &&
          (action.allDay === undefined || raw.allDay === action.allDay) &&
          (action.start === undefined ||
            Date.parse(raw.start ?? "") === Date.parse(action.start)) &&
          (action.end === undefined || Date.parse(raw.end ?? "") === Date.parse(action.end));
    if (!verified) {
      const detail =
        "The native readback differs from the approved change. Inspect the item before retrying.";
      const result = raw === null ? null : NativeHost.nativeRecord(raw);
      yield* sql`UPDATE heiwa_native_proposals SET status = 'uncertain', result_json = ${encodeJson(result)}, detail = ${detail} WHERE id = ${id}`;
      return { ...(yield* decodeRow(row)), status: "uncertain" as const, detail, result };
    }
    const result = raw === null ? null : NativeHost.nativeRecord(raw);
    yield* sql`UPDATE heiwa_native_proposals SET status = 'applied', result_json = ${encodeJson(result)} WHERE id = ${id}`;
    return { ...(yield* decodeRow(row)), status: "applied" as const, result };
  });
  const currentStatus = status().pipe(Effect.mapError(storageError));
  return {
    status: currentStatus,
    subscribeStatus: Stream.unwrap(
      Effect.gen(function* () {
        // Subscribe before reading so a change during the read is not lost.
        const subscription = yield* PubSub.subscribe(changes);
        return Stream.concat(
          Stream.fromEffect(currentStatus),
          Stream.fromSubscription(subscription).pipe(Stream.mapEffect(() => currentStatus)),
        );
      }),
    ),
    inventory: host.inventory,
    select: Effect.fn("NativeWorkflows.select")(
      function* (
        app: typeof NativeApp.Type,
        resourceIds: ReadonlyArray<string>,
        writeMode?: typeof NativeWriteMode.Type,
      ) {
        if (new Set(resourceIds).size !== resourceIds.length)
          return yield* failure("invalid", "A resource can only be selected once.");
        const current = resourceIds.length > 0 ? yield* host.inventory(app, false) : [];
        if (resourceIds.length > 0) {
          if (resourceIds.some((id) => !current.some((row) => row.id === id)))
            return yield* failure("selection", "A selected resource is no longer available.");
        }
        const prior = (yield* grants()).find((row) => row.app === app);
        const mode = writeMode ?? prior?.writeMode ?? "review";
        yield* sql`INSERT INTO heiwa_native_grants (app, resource_ids_json, resources_json, write_mode) VALUES (${app}, ${encodeJson(resourceIds)}, ${encodeJson(current.filter((row) => resourceIds.includes(row.id)))}, ${mode}) ON CONFLICT(app) DO UPDATE SET resource_ids_json = excluded.resource_ids_json, resources_json = excluded.resources_json, write_mode = excluded.write_mode`;
        return yield* status();
      },
      (effect) => announcing(guarded(gate.withPermits(1)(effect))),
    ),
    read: (input: NativeReadInput) => guarded(gate.withPermits(1)(read(input))),
    propose: (action: typeof NativeAction.Type, origin?: string) =>
      announcing(guarded(gate.withPermits(1)(propose(action, origin)))),
    approve: (id: string, value: boolean) =>
      announcing(guarded(gate.withPermits(1)(approve(id, value)))),
    execute: Effect.fn("NativeWorkflows.execute")(
      function* (action: typeof NativeAction.Type) {
        const authority = yield* NativeExecutionAuthority;
        return yield* gate.withPermits(1)(
          Effect.gen(function* () {
            yield* authority.check;
            const grant = (yield* grants()).find((row) => row.app === action.app);
            if (
              grant?.writeMode !== "automatic" ||
              action.operation === "trash" ||
              (action.app === "notes" && action.operation === "update" && action.body !== undefined)
            )
              return yield* reviewRequired();
            const proposal = yield* propose(action, authority.origin);
            yield* authority.check;
            return yield* approve(proposal.id, true);
          }),
        );
      },
      guarded,
      announcing,
    ),
    resolve: Effect.fn("NativeWorkflows.resolve")(
      function* (id: string, note: string) {
        const rows = yield* sql<ProposalRow>`SELECT * FROM heiwa_native_proposals WHERE id = ${id}`;
        const row = rows[0];
        if (!row || !["uncertain", "applying", "failed"].includes(row.status))
          return yield* failure(
            "invalid",
            "Only a failed or uncertain change can be acknowledged.",
          );
        const detail = `Owner checked the native outcome: ${note}`;
        yield* sql`UPDATE heiwa_native_proposals SET status = 'acknowledged', detail = ${detail} WHERE id = ${id}`;
        return { ...(yield* decodeRow(row)), status: "acknowledged" as const, detail };
      },
      (effect) => announcing(guarded(gate.withPermits(1)(effect))),
    ),
    run: Effect.fn("NativeWorkflows.run")(
      function* (input: typeof NativeWorkflowInput.Type) {
        if (input.workflow === "native-audit")
          return {
            workflow: input.workflow,
            status: { ...(yield* status()), pending: [], recent: [] },
            sources: [],
          };
        const window =
          input.start === undefined && input.end === undefined
            ? nativeDayWindow(yield* DateTime.now, DateTime.zoneMakeLocal())
            : { start: input.start, end: input.end };
        const sources = yield* Effect.forEach(["calendar", "reminders", "mail"] as const, (app) =>
          read({ app, ...window, limit: input.limit ?? 10 }).pipe(
            Effect.map((result) => ({ app, ...result, error: null })),
            Effect.catch((error) =>
              Effect.succeed({
                app,
                records: [],
                truncated: false,
                error: isNativeWorkflowError(error)
                  ? error.detail
                  : "This source could not be read.",
              }),
            ),
          ),
        );
        return {
          workflow: input.workflow,
          status: { ...(yield* status()), pending: [], recent: [] },
          sources,
        };
      },
      (effect) => guarded(gate.withPermits(1)(effect)),
    ),
  };
});
export class NativeWorkflows extends Context.Service<
  NativeWorkflows,
  Effect.Success<typeof make>
>()("t3/native/NativeWorkflows") {}
export const layer = Layer.effect(NativeWorkflows, make);
