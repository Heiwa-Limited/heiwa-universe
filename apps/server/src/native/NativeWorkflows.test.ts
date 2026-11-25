// @effect-diagnostics nodeBuiltinImport:off -- Mail intake runs against a synthetic Envelope Index.
import * as NodeSqlite from "node:sqlite";
import { assert, describe, it } from "@effect/vitest";
import * as NodeSqliteClient from "@t3tools/shared/nodeSqliteClient";
import { NativeWorkflowError } from "@t3tools/contracts";
import * as DateTime from "effect/DateTime";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as Queue from "effect/Queue";
import * as SqlClient from "effect/sql/SqlClient";
import * as Stream from "effect/Stream";
import * as MailIndex from "./MailIndex.ts";
import * as NativeHost from "./NativeHost.ts";
import * as NativeWorkflows from "./NativeWorkflows.ts";
import authorityMigration from "../persistence/Migrations/077_NativeWorkflowAuthority.ts";
import migration from "../persistence/Migrations/076_HeiwaNativeWorkflows.ts";
import sourceMigration from "../persistence/Migrations/078_NativeProposalSource.ts";
import intakeMigration from "../persistence/Migrations/079_NativeIntakeCursor.ts";
import watchMigration from "../persistence/Migrations/080_NativeWatchCheck.ts";
import keysetMigration from "../persistence/Migrations/081_NativeIntakeKeyset.ts";

const snapshot: NativeHost.NativeSnapshot = {
  app: "reminders",
  id: "reminder-1",
  resourceId: "list-1",
  title: "Before",
  body: "",
  start: null,
  end: null,
  completed: false,
  modifiedAt: "2026-10-30T10:00:00Z",
  recurring: false,
  hasAttendees: false,
};
const resource = { app: "reminders" as const, id: "list-1", title: "Test list", writable: true };
const INBOX = "imap://ACCT/INBOX";
const mailResource = { app: "mail" as const, id: INBOX, title: "INBOX", writable: true };
const fixture = (apply?: NativeHost.NativeHost["Service"]["apply"]) => {
  let writes = 0;
  let applied: ReadonlyArray<string> = [];
  let mailDenied = false;
  let currentResource = resource;
  const envelope = new NodeSqlite.DatabaseSync(":memory:");
  envelope.exec(`
    CREATE TABLE mailboxes (ROWID INTEGER PRIMARY KEY, url TEXT, unread_count INTEGER);
    CREATE TABLE subjects (ROWID INTEGER PRIMARY KEY, subject TEXT);
    CREATE TABLE summaries (ROWID INTEGER PRIMARY KEY, summary TEXT);
    CREATE TABLE addresses (ROWID INTEGER PRIMARY KEY, address TEXT, comment TEXT);
    CREATE TABLE recipients (ROWID INTEGER PRIMARY KEY, message INTEGER, address INTEGER, type INTEGER, position INTEGER);
    CREATE TABLE messages (ROWID INTEGER PRIMARY KEY, sender INTEGER, subject_prefix TEXT, subject INTEGER,
      summary INTEGER, date_received INTEGER, mailbox INTEGER, read INTEGER, deleted INTEGER);
    INSERT INTO mailboxes VALUES (1, '${INBOX}', 0), (2, 'imap://ACCT/Archive', 0);
    INSERT INTO subjects VALUES (1, 'Hello');
    INSERT INTO addresses VALUES (1, 'sender@example.com', 'Sender');
  `);
  const denied = () =>
    new NativeWorkflowError({ code: "permission", detail: "Allow Heiwa to control Mail." });
  const host = NativeHost.NativeHost.of({
    platform: "darwin",
    inventory: (app) => Effect.succeed(app === "mail" ? [mailResource] : [currentResource]),
    read: (input, ids) => {
      if (input.app !== "mail")
        return Effect.succeed({
          records: ids.includes("list-1") ? [snapshot] : [],
          truncated: false,
        });
      if (mailDenied) return Effect.fail(denied());
      return Effect.sync(() =>
        MailIndex.messages(envelope, {
          urls: ids,
          limit: input.limit ?? 20,
          includeRead: input.includeCompleted ?? false,
          start: input.start,
          end: input.end,
        }),
      );
    },
    mailIntake: (after, limit, ids) =>
      mailDenied
        ? Effect.fail(denied())
        : Effect.sync(() => MailIndex.intake(envelope, { urls: ids, after, limit })),
    inspect: () => Effect.succeed(snapshot),
    apply: (action, expected) => {
      writes += 1;
      applied = Object.keys(action);
      return apply
        ? apply(action, expected)
        : Effect.succeed({
            ...snapshot,
            title: action.title ?? snapshot.title,
            completed: action.completed ?? snapshot.completed,
          });
    },
  });
  return {
    host,
    writes: () => writes,
    appliedFields: () => applied,
    /** Adds a message to Mail's index and returns its ROWID as a record id. */
    deliverMail: (
      received: string,
      { body = "", read = false, mailbox = 1 }: { body?: string; read?: boolean; mailbox?: number } = {},
    ) => {
      const summary = envelope.prepare("INSERT INTO summaries (summary) VALUES (?)").run(body);
      const row = envelope
        .prepare(
          "INSERT INTO messages (sender, subject_prefix, subject, summary, date_received, mailbox, read, deleted) VALUES (1, NULL, 1, ?, ?, ?, ?, 0)",
        )
        .run(summary.lastInsertRowid, Date.parse(received) / 1000, mailbox, read ? 1 : 0);
      return String(row.lastInsertRowid);
    },
    denyMail: (value: boolean) => {
      mailDenied = value;
    },
    renameResource: () => {
      currentResource = { ...resource, title: "Renamed list" };
    },
    layer: Layer.effectDiscard(
      migration.pipe(
        Effect.andThen(authorityMigration),
        Effect.andThen(sourceMigration),
        Effect.andThen(intakeMigration),
        Effect.andThen(watchMigration),
        Effect.andThen(keysetMigration),
      ),
    ).pipe(
      Layer.provideMerge(
        NativeWorkflows.layer.pipe(Layer.provide(Layer.succeed(NativeHost.NativeHost, host))),
      ),
      Layer.provideMerge(NodeSqliteClient.layer({ filename: ":memory:" })),
    ),
  };
};
const create = {
  app: "reminders" as const,
  operation: "create" as const,
  resourceId: "list-1",
  title: "After",
  body: "",
};
const selected = Effect.gen(function* () {
  const service = yield* NativeWorkflows.NativeWorkflows;
  yield* service.select("reminders", ["list-1"]);
  return service;
});

describe("native workflow authority", () => {
  it.effect(
    "reads only selected lists and revokes pending writes when selection is removed",
    () => {
      const f = fixture();
      return Effect.gen(function* () {
        const service = yield* NativeWorkflows.NativeWorkflows;
        assert.deepStrictEqual(yield* service.read({ app: "reminders" }), {
          records: [],
          truncated: false,
        });
        yield* service.select("reminders", ["list-1"]);
        const proposal = yield* service.propose(create);
        yield* service.select("reminders", []);
        assert.equal(
          (yield* service.approve(proposal.id, true).pipe(Effect.result))._tag,
          "Failure",
        );
        assert.equal(f.writes(), 0);
      }).pipe(Effect.provide(f.layer));
    },
  );
  it.effect(
    "rechecks the named resource before applying and exposes the reviewed before state",
    () => {
      const f = fixture();
      return Effect.gen(function* () {
        const service = yield* selected;
        const proposal = yield* service.propose(create);
        assert.equal(proposal.resource?.title, "Test list");
        f.renameResource();
        assert.equal(
          (yield* service.approve(proposal.id, true).pipe(Effect.result))._tag,
          "Failure",
        );
        assert.equal(f.writes(), 0);
      }).pipe(Effect.provide(f.layer));
    },
  );
  it.effect("automatic changes require owner opt-in and revalidate live chat authority", () => {
    const f = fixture();
    let checks = 0;
    const authority = {
      origin: "test-chat",
      check: Effect.sync(() => {
        checks++;
      }),
    };
    return Effect.gen(function* () {
      const service = yield* selected;
      const execute = service
        .execute(create)
        .pipe(Effect.provideService(NativeWorkflows.NativeExecutionAuthority, authority));
      assert.equal((yield* execute.pipe(Effect.result))._tag, "Failure");
      yield* service.select("reminders", ["list-1"], "automatic");
      assert.equal((yield* execute).status, "applied");
      assert.equal(checks, 3);
      assert.equal(f.writes(), 1);
      const denied = service.execute(create).pipe(
        Effect.provideService(NativeWorkflows.NativeExecutionAuthority, {
          origin: "revoked",
          check: Effect.fail(new NativeWorkflowError({ code: "permission", detail: "Revoked" })),
        }),
      );
      assert.equal((yield* denied.pipe(Effect.result))._tag, "Failure");
      assert.equal(f.writes(), 1);
      assert.equal(
        (yield* service
          .execute({
            ...create,
            operation: "trash",
            id: snapshot.id,
            expectedRevision: NativeHost.nativeRecord(snapshot).revision,
          })
          .pipe(
            Effect.provideService(NativeWorkflows.NativeExecutionAuthority, authority),
            Effect.result,
          ))._tag,
        "Failure",
      );
      assert.equal(f.writes(), 1);
    }).pipe(Effect.provide(f.layer));
  });
  it.effect("acknowledges an uncertain result without retrying its operation", () => {
    const f = fixture(() =>
      Effect.fail(new NativeWorkflowError({ code: "uncertain", detail: "Unknown outcome" })),
    );
    return Effect.gen(function* () {
      const service = yield* selected;
      const proposal = yield* service.propose(create);
      yield* service.approve(proposal.id, true);
      const resolved = yield* service.resolve(
        proposal.id,
        "Inspected the test list; no duplicate exists.",
      );
      assert.equal(resolved.status, "acknowledged");
      assert.equal((yield* service.approve(proposal.id, true)).status, "acknowledged");
      assert.equal(f.writes(), 1);
    }).pipe(Effect.provide(f.layer));
  });
  it.effect("streams the pending list as proposals arrive and are answered", () => {
    const f = fixture();
    return Effect.gen(function* () {
      const service = yield* selected;
      const seen = yield* Queue.unbounded<number>();
      yield* service.subscribeStatus.pipe(
        Stream.runForEach((status) => Queue.offer(seen, status.pending.length)),
        Effect.forkScoped,
      );
      assert.equal(yield* Queue.take(seen), 0);
      const proposal = yield* service.propose(create);
      assert.equal(yield* Queue.take(seen), 1);
      yield* service.approve(proposal.id, false);
      assert.equal(yield* Queue.take(seen), 0);
    }).pipe(Effect.scoped, Effect.provide(f.layer));
  });
  it.effect("names a change's source from the item itself and keeps it out of the write", () => {
    const f = fixture();
    return Effect.gen(function* () {
      const service = yield* selected;
      const source = { app: "reminders" as const, resourceId: "list-1", id: "reminder-1" };
      const proposal = yield* service.propose({ ...create, source });
      assert.deepStrictEqual(proposal.source, { ...source, title: "Before" });
      assert.deepStrictEqual((yield* service.status).pending[0]?.source, proposal.source);
      // Staging the same change again returns the first instead of adding a second entry.
      assert.equal((yield* service.propose({ ...create, source })).id, proposal.id);
      assert.equal((yield* service.status).pending.length, 1);
      yield* service.approve(proposal.id, true);
      assert.isFalse(f.appliedFields().includes("source"));
      assert.equal(
        (yield* service
          .propose({ ...create, source: { ...source, resourceId: "list-2" } })
          .pipe(Effect.result))._tag,
        "Failure",
      );
    }).pipe(Effect.provide(f.layer));
  });
  it.effect("hands each new message to intake once, including late arrivals in a handed-out second", () => {
    const f = fixture();
    return Effect.gen(function* () {
      const service = yield* NativeWorkflows.NativeWorkflows;
      const sql = yield* SqlClient.SqlClient;
      const intake = () =>
        service.run({ workflow: "mail-intake" }).pipe(Effect.map((result) => result.sources[0]!));
      const ids = () =>
        intake().pipe(Effect.map((source) => source.records.map((record) => record.id)));
      const watched = () => service.status.pipe(Effect.map((status) => status.mailWatch));
      assert.equal(yield* watched(), null);
      // An empty watch stays closed: nothing selected, then nothing unread in the selection.
      assert.equal(yield* service.hasNewMail, false);
      yield* service.select("mail", [INBOX]);
      assert.equal(yield* service.hasNewMail, false);
      assert.equal((yield* watched())?.outcome, "no-new-mail");
      f.deliverMail("2026-11-14T10:00:00Z", { read: true });
      f.deliverMail("2026-11-14T10:00:00Z", { mailbox: 2 });
      assert.equal(yield* service.hasNewMail, false);
      const first = f.deliverMail("2026-11-14T10:00:00Z");
      assert.equal(yield* service.hasNewMail, true);
      assert.equal((yield* watched())?.outcome, "new-mail");
      assert.deepStrictEqual(yield* ids(), [first]);
      assert.equal(yield* service.hasNewMail, false);
      // Mail that cannot be read keeps the watch closed, says why, and leaves the cursor alone.
      const sameSecond = f.deliverMail("2026-11-14T10:00:00Z");
      f.denyMail(true);
      assert.equal(yield* service.hasNewMail, false);
      assert.deepInclude(yield* watched(), {
        outcome: "unreadable",
        detail: "Allow Heiwa to control Mail.",
      });
      const failed = yield* intake();
      assert.deepStrictEqual([failed.records, failed.error], [[], "Allow Heiwa to control Mail."]);
      f.denyMail(false);
      // Indexed after its second was handed out, it still arrives, and only once.
      assert.equal(yield* service.hasNewMail, true);
      assert.deepStrictEqual(yield* ids(), [sameSecond]);
      assert.deepStrictEqual(yield* ids(), []);
      assert.deepStrictEqual(yield* sql`SELECT after, after_id FROM heiwa_native_intake`, [
        { after: "2026-11-14T10:00:00Z", after_id: Number(sameSecond) },
      ]);
    }).pipe(Effect.provide(f.layer));
  });
  it.effect("resumes a cursor saved before keyset intake from its second", () => {
    const f = fixture();
    return Effect.gen(function* () {
      const service = yield* NativeWorkflows.NativeWorkflows;
      const sql = yield* SqlClient.SqlClient;
      yield* service.select("mail", [INBOX]);
      // The earlier intake stored the first second it had not handed out, without an id.
      yield* sql`INSERT INTO heiwa_native_intake (app, after) VALUES ('mail', '2026-11-14T10:00:05Z')`;
      f.deliverMail("2026-11-14T10:00:04Z");
      const expected = [
        f.deliverMail("2026-11-14T10:00:05Z"),
        f.deliverMail("2026-11-14T10:00:05Z"),
        f.deliverMail("2026-11-14T10:00:06Z"),
      ];
      assert.equal(yield* service.hasNewMail, true);
      const source = (yield* service.run({ workflow: "mail-intake" })).sources[0]!;
      assert.deepStrictEqual(
        source.records.map((record) => record.id),
        expected,
      );
      assert.isFalse(source.truncated);
      assert.deepStrictEqual(yield* sql`SELECT after, after_id FROM heiwa_native_intake`, [
        { after: "2026-11-14T10:00:06Z", after_id: Number(expected[2]) },
      ]);
    }).pipe(Effect.provide(f.layer));
  });
  it.effect(
    "drains a backlog larger than a page oldest first within the byte budget, across a restart",
    () => {
      const f = fixture();
      const utf8 = new TextEncoder();
      const long = "x".repeat(3000);
      // Delivered newest first so ROWIDs disagree with arrival order; 14 messages share a second.
      const seconds = [
        ...Array.from({ length: 4 }, () => "2026-11-14T10:00:09Z"),
        ...Array.from({ length: 14 }, () => "2026-11-14T10:00:05Z"),
        ...Array.from({ length: 3 }, () => "2026-11-14T10:00:00Z"),
      ];
      const delivered = seconds.map((received) => ({
        received,
        id: f.deliverMail(received, { body: long }),
      }));
      const expected = [...delivered]
        .sort((a, b) => a.received.localeCompare(b.received) || Number(a.id) - Number(b.id))
        .map((entry) => entry.id);
      const tied = new Set(
        delivered
          .filter((entry) => entry.received === "2026-11-14T10:00:05Z")
          .map((entry) => entry.id),
      );
      const batches: Array<ReadonlyArray<string>> = [];
      // Runs intake until it is empty, checking each batch and that the watch agrees with it.
      const drain = (runs: number, limit?: number) =>
        Effect.gen(function* () {
          const service = yield* NativeWorkflows.NativeWorkflows;
          for (let run = 0; run < runs; run++) {
            const open = yield* service.hasNewMail;
            const source = (yield* service.run({
              workflow: "mail-intake",
              ...(limit === undefined ? {} : { limit }),
            })).sources[0]!;
            assert.equal(open, source.records.length > 0);
            if (source.records.length === 0) return;
            if (limit !== undefined) assert.lengthOf(source.records, limit);
            const bytes = source.records.reduce(
              (sum, record) => sum + utf8.encode(JSON.stringify(record)).length,
              0,
            );
            assert.isAtMost(bytes, 6_000);
            assert.isTrue(source.records.every((record) => record.body.length <= 1_001));
            assert.isTrue(source.truncated);
            batches.push(source.records.map((record) => record.id));
          }
        });
      const restarted = NativeWorkflows.layer.pipe(
        Layer.provide(Layer.succeed(NativeHost.NativeHost, f.host)),
      );
      return Effect.gen(function* () {
        const service = yield* NativeWorkflows.NativeWorkflows;
        yield* service.select("mail", [INBOX]);
        yield* drain(1, 2);
        yield* drain(2);
        // A run that cannot read Mail hands nothing out and moves nothing.
        f.denyMail(true);
        const failed = (yield* service.run({ workflow: "mail-intake" })).sources[0]!;
        assert.isNotNull(failed.error);
        f.denyMail(false);
        // A fresh service over the same database resumes from the saved cursor.
        yield* drain(30).pipe(Effect.provide(restarted));
        assert.deepStrictEqual(batches.flat(), expected);
        assert.isAbove(batches.filter((batch) => batch.some((id) => tied.has(id))).length, 1);
        assert.equal(yield* service.hasNewMail, false);
      }).pipe(Effect.provide(f.layer));
    },
  );
  it.effect("requires exact current revision before staging an edit", () => {
    const f = fixture();
    return Effect.gen(function* () {
      const service = yield* selected;
      assert.equal(
        (yield* service
          .propose({
            ...create,
            operation: "update",
            id: snapshot.id,
            expectedRevision: "outdated",
          })
          .pipe(Effect.result))._tag,
        "Failure",
      );
      const proposal = yield* service.propose({
        ...create,
        operation: "update",
        id: snapshot.id,
        expectedRevision: NativeHost.nativeRecord(snapshot).revision,
      });
      assert.equal(proposal.status, "pending");
      assert.equal(f.writes(), 0);
    }).pipe(Effect.provide(f.layer));
  });
  it.effect("persists readback and never repeats a completed write", () => {
    const f = fixture();
    return Effect.gen(function* () {
      const service = yield* selected;
      const proposal = yield* service.propose(create);
      const result = yield* service.approve(proposal.id, true);
      assert.equal(result.status, "applied");
      assert.equal(result.result?.title, "After");
      assert.deepStrictEqual(yield* service.approve(proposal.id, true), result);
      assert.equal(f.writes(), 1);
    }).pipe(Effect.provide(f.layer));
  });
  it.effect("does not retry a timed out write or a proposal left applying after a crash", () => {
    const f = fixture(() =>
      Effect.fail(new NativeWorkflowError({ code: "uncertain", detail: "timed out" })),
    );
    return Effect.gen(function* () {
      const service = yield* selected;
      const proposal = yield* service.propose(create);
      assert.equal((yield* service.approve(proposal.id, true)).status, "uncertain");
      assert.equal((yield* service.approve(proposal.id, true).pipe(Effect.result))._tag, "Failure");
      const crashed = yield* service.propose(create);
      const sql = yield* SqlClient.SqlClient;
      yield* sql`UPDATE heiwa_native_proposals SET status = 'applying' WHERE id = ${crashed.id}`;
      assert.equal((yield* service.approve(crashed.id, true).pipe(Effect.result))._tag, "Failure");
      assert.equal(f.writes(), 1);
    }).pipe(Effect.provide(f.layer));
  });
  it.effect("marks a mismatched native readback uncertain", () => {
    const f = fixture(() => Effect.succeed(snapshot));
    return Effect.gen(function* () {
      const service = yield* selected;
      const proposal = yield* service.propose(create);
      assert.equal((yield* service.approve(proposal.id, true)).status, "uncertain");
    }).pipe(Effect.provide(f.layer));
  });
  it.effect("rejects invalid cross-app commands before they reach the native helper", () => {
    const f = fixture();
    return Effect.gen(function* () {
      const service = yield* selected;
      for (const action of [
        { ...create, completed: true },
        { ...create, operation: "complete" as const, completed: true },
        { ...create, start: "2026-10-30T10:00:00Z" },
      ]) {
        assert.equal((yield* service.propose(action).pipe(Effect.result))._tag, "Failure");
      }
      assert.equal(f.writes(), 0);
    }).pipe(Effect.provide(f.layer));
  });
});

it("rejects impossible calendar dates and timestamps without an explicit time zone", () => {
  for (const start of ["2026-02-30T10:00:00Z", "2026-10-30T10:00:00", "2026-13-01T10:00:00Z"]) {
    assert.isFalse(
      NativeWorkflows.validateAction({
        app: "calendar",
        operation: "create",
        resourceId: "calendar",
        title: "Fixture",
        start,
        end: "2026-12-01T11:00:00Z",
      }),
    );
  }
});

it("daily brief uses the Mac day boundary across daylight-saving changes", () => {
  const zone = DateTime.zoneMakeNamedUnsafe("America/Vancouver");
  assert.deepStrictEqual(
    NativeWorkflows.nativeDayWindow(DateTime.makeUnsafe("2026-03-08T18:00:00Z"), zone),
    { start: "2026-03-08T08:00:00Z", end: "2026-03-09T07:00:00Z" },
  );
});

it("gives a timed event without an end one hour in its own time zone", () => {
  const event = {
    app: "calendar" as const,
    operation: "create" as const,
    resourceId: "calendar-1",
    title: "Dinner",
    start: "2026-11-19T23:30:00-05:00",
  };
  assert.equal(NativeWorkflows.withDefaultEnd(event).end, "2026-11-20T00:30:00-05:00");
  assert.equal(
    NativeWorkflows.withDefaultEnd({ ...event, end: "2026-11-20T02:00:00-05:00" }).end,
    "2026-11-20T02:00:00-05:00",
  );
  assert.isUndefined(NativeWorkflows.withDefaultEnd({ ...event, allDay: true }).end);
});
it("validates reminder due dates and prevents cross-app reminder metadata", () => {
  assert.isTrue(NativeWorkflows.validateAction({ ...create, due: "2026-11-02", priority: 1 }));
  assert.isTrue(NativeWorkflows.validateAction({ ...create, due: null }));
  assert.isFalse(NativeWorkflows.validateAction({ ...create, due: "2026-02-30" }));
  assert.isFalse(NativeWorkflows.validateAction({ ...create, allDay: true }));
  assert.isFalse(NativeWorkflows.validateAction({ ...create, app: "notes", priority: 1 }));
});

it("limits Mail to drafts, read state and Trash, and keeps recipients on Mail drafts", () => {
  const draft = {
    app: "mail" as const,
    operation: "create" as const,
    resourceId: "imap://ACCOUNT/INBOX",
    title: "Subject",
    body: "Text",
    recipients: ["someone@example.com"],
  };
  const existing = { id: "42", expectedRevision: "rev" };
  assert.isTrue(NativeWorkflows.validateAction(draft));
  assert.isTrue(
    NativeWorkflows.validateAction({
      ...existing,
      app: "mail",
      operation: "complete",
      resourceId: draft.resourceId,
      completed: true,
    }),
  );
  assert.isTrue(
    NativeWorkflows.validateAction({
      ...existing,
      app: "mail",
      operation: "trash",
      resourceId: draft.resourceId,
    }),
  );
  assert.isFalse(
    NativeWorkflows.validateAction({
      ...existing,
      app: "mail",
      operation: "update",
      resourceId: draft.resourceId,
      title: "Rewritten received mail",
    }),
  );
  assert.isFalse(NativeWorkflows.validateAction({ ...create, recipients: ["a@example.com"] }));
  assert.isTrue(NativeWorkflows.sameMailAccount("imap://A/INBOX", "imap://A/%5BGmail%5D/Drafts"));
  assert.isFalse(NativeWorkflows.sameMailAccount("imap://A/INBOX", "imap://B/INBOX"));
});
