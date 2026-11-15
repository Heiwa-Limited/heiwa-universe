import * as DateTime from "effect/DateTime";
import * as Schema from "effect/Schema";

/** Civil-date round trip rejects Date.parse's silent normalization of impossible days. */
export function isNativeInstant(value: string): boolean {
  if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:Z|[+-]\d{2}:\d{2})$/.test(value)) return false;
  const civil = value.slice(0, 19);
  const parsed = Date.parse(`${civil}Z`);
  if (
    !Number.isFinite(parsed) ||
    DateTime.formatIso(DateTime.makeUnsafe(parsed)).slice(0, 19) !== civil
  )
    return false;
  const zone = value.slice(19);
  return zone === "Z" || (Number(zone.slice(1, 3)) <= 23 && Number(zone.slice(4, 6)) <= 59);
}
const Instant = Schema.String.check(
  Schema.makeFilter(isNativeInstant, {
    expected: "an explicit ISO timestamp with a valid civil date and time zone",
  }),
);
const Id = Schema.String.check(Schema.isMinLength(1), Schema.isMaxLength(1024));
const Text = Schema.String.check(Schema.isMaxLength(16000));
export const NativeApp = Schema.Literals(["calendar", "reminders", "notes", "mail"]);
/** Mail addresses only; display names are never accepted from agents. */
const Email = Schema.String.check(
  Schema.isMaxLength(320),
  Schema.isPattern(/^[^\s@<>,;"]+@[^\s@<>,;"]+\.[^\s@<>,;"]+$/),
);
export type NativeApp = typeof NativeApp.Type;
export const NativeResource = Schema.Struct({
  app: NativeApp,
  id: Id,
  title: Text,
  source: Schema.optional(Text),
  writable: Schema.Boolean,
});
export type NativeResource = typeof NativeResource.Type;
export const NativeRecord = Schema.Struct({
  app: NativeApp,
  id: Id,
  resourceId: Id,
  title: Text,
  body: Text,
  start: Schema.NullOr(Schema.String),
  end: Schema.NullOr(Schema.String),
  completed: Schema.Boolean,
  due: Schema.optional(Schema.NullOr(Schema.String)),
  priority: Schema.optional(Schema.Number),
  allDay: Schema.optional(Schema.Boolean),
  locked: Schema.optional(Schema.Boolean),
  /** Mail: sender as shown by Mail and To recipients. `completed` is the read state. */
  sender: Schema.optional(Text),
  recipients: Schema.optional(Schema.Array(Text).check(Schema.isMaxLength(50))),
  /** Mail: full message text, fetched only for reads by id. Not part of the revision. */
  content: Schema.optional(Text),
  modifiedAt: Schema.NullOr(Schema.String),
  revision: Id,
});
export type NativeRecord = typeof NativeRecord.Type;
export const NativeReadInput = Schema.Struct({
  app: NativeApp,
  start: Schema.optional(Instant),
  end: Schema.optional(Instant),
  includeCompleted: Schema.optional(Schema.Boolean),
  limit: Schema.optional(Schema.Int.check(Schema.isBetween({ minimum: 1, maximum: 100 }))),
  query: Schema.optional(
    Schema.String.check(Schema.isMinLength(1), Schema.isMaxLength(200)).annotate({
      description: "Mail only: subject or sender text.",
    }),
  ),
  ids: Schema.optional(
    Schema.Array(Id).check(Schema.isMinLength(1), Schema.isMaxLength(5)).annotate({
      description: "Mail only: ids from an earlier read, for full text.",
    }),
  ),
});
export type NativeReadInput = typeof NativeReadInput.Type;
export const NativeReadResult = Schema.Struct({
  records: Schema.Array(NativeRecord),
  truncated: Schema.Boolean,
});
/** The item a change answers, such as the email an event or a reply came from. */
const NativeSourceRef = Schema.Struct({ app: NativeApp, resourceId: Id, id: Id });
export const NativeSource = Schema.Struct({
  ...NativeSourceRef.fields,
  title: Text,
  sender: Schema.optional(Text),
});
export type NativeSource = typeof NativeSource.Type;
/** A change without its source: what the Apple write receives and what automatic changes take. */
export const NativeChange = Schema.Struct({
  app: NativeApp,
  operation: Schema.Literals(["create", "update", "append", "complete", "trash"]),
  resourceId: Id,
  id: Schema.optional(Id),
  expectedRevision: Schema.optional(Id),
  title: Schema.optional(Text),
  body: Schema.optional(Text),
  start: Schema.optional(Instant),
  end: Schema.optional(Instant),
  completed: Schema.optional(Schema.Boolean),
  due: Schema.optional(Schema.NullOr(Schema.String.check(Schema.isMaxLength(40)))),
  priority: Schema.optional(Schema.Literals([0, 1, 5, 9])),
  allDay: Schema.optional(Schema.Boolean),
  /** Mail drafts only. Heiwa saves drafts; sending stays in Mail. */
  recipients: Schema.optional(Schema.Array(Email).check(Schema.isMaxLength(20))),
});
export type NativeChange = typeof NativeChange.Type;
export const NativeAction = Schema.Struct({
  ...NativeChange.fields,
  source: Schema.optional(
    NativeSourceRef.annotate({
      description: "Item this change answers, from a read, such as the email it came from.",
    }),
  ),
});
export type NativeAction = typeof NativeAction.Type;
export const NativeProposal = Schema.Struct({
  id: Id,
  action: NativeAction,
  resource: Schema.NullOr(NativeResource),
  before: Schema.NullOr(NativeRecord),
  source: Schema.NullOr(NativeSource),
  status: Schema.Literals([
    "pending",
    "applying",
    "applied",
    "uncertain",
    "failed",
    "rejected",
    "acknowledged",
  ]),
  createdAt: Schema.String,
  result: Schema.NullOr(NativeRecord),
  detail: Schema.NullOr(Text),
});
export type NativeProposal = typeof NativeProposal.Type;
export const NativeWriteMode = Schema.Literals(["review", "automatic"]);
export const NativeStatus = Schema.Struct({
  platform: Schema.String,
  grants: Schema.Array(
    Schema.Struct({
      app: NativeApp,
      resourceIds: Schema.Array(Id),
      resources: Schema.Array(NativeResource),
      writeMode: NativeWriteMode,
    }),
  ),
  pending: Schema.Array(NativeProposal),
  recent: Schema.Array(NativeProposal),
});
export const NativeInventoryInput = Schema.Struct({
  app: NativeApp,
  requestAccess: Schema.Boolean,
});
export const NativeSelectionInput = Schema.Struct({
  app: NativeApp,
  resourceIds: Schema.Array(Id).check(Schema.isMaxLength(100)),
  writeMode: Schema.optional(NativeWriteMode),
});
export const NativeResolutionInput = Schema.Struct({
  id: Id,
  note: Schema.String.check(Schema.isMinLength(1), Schema.isMaxLength(1000)),
});
export const NativeApprovalInput = Schema.Struct({ id: Id, approve: Schema.Boolean });
export const NativeWorkflowInput = Schema.Struct({
  workflow: Schema.Literals(["daily-brief", "native-audit"]),
  start: Schema.optional(Instant),
  end: Schema.optional(Instant),
  limit: Schema.optional(Schema.Int.check(Schema.isBetween({ minimum: 1, maximum: 50 }))),
});
export class NativeWorkflowError extends Schema.TaggedError<NativeWorkflowError>()(
  "NativeWorkflowError",
  {
    code: Schema.Literals([
      "unavailable",
      "permission",
      "selection",
      "invalid",
      "stale",
      "uncertain",
      "failed",
    ]),
    detail: Text,
  },
) {
  override get message() {
    return this.detail;
  }
}
export const NativeWorkflowResult = Schema.Struct({
  workflow: Schema.String,
  status: NativeStatus,
  sources: Schema.Array(
    Schema.Struct({
      app: NativeApp,
      records: Schema.Array(NativeRecord),
      truncated: Schema.Boolean,
      error: Schema.NullOr(Schema.String),
    }),
  ),
});
