import { assert, describe, it } from "@effect/vitest";
import * as NodeSqliteClient from "@t3tools/shared/nodeSqliteClient";
import { NativeWorkflowError } from "@t3tools/contracts";
import * as DateTime from "effect/DateTime";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as Queue from "effect/Queue";
import * as SqlClient from "effect/sql/SqlClient";
import * as Stream from "effect/Stream";
import * as NativeHost from "./NativeHost.ts";
import * as NativeWorkflows from "./NativeWorkflows.ts";
import authorityMigration from "../persistence/Migrations/077_NativeWorkflowAuthority.ts";
import migration from "../persistence/Migrations/076_HeiwaNativeWorkflows.ts";

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
const fixture = (apply?: NativeHost.NativeHost["Service"]["apply"]) => {
  let writes = 0;
  let currentResource = resource;
  const host = NativeHost.NativeHost.of({
    platform: "darwin",
    inventory: () => Effect.succeed([currentResource]),
    read: (_input, ids) =>
      Effect.succeed({ records: ids.includes("list-1") ? [snapshot] : [], truncated: false }),
    inspect: () => Effect.succeed(snapshot),
    apply: (action, expected) => {
      writes += 1;
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
    writes: () => writes,
    renameResource: () => {
      currentResource = { ...resource, title: "Renamed list" };
    },
    layer: Layer.effectDiscard(Effect.andThen(migration, authorityMigration)).pipe(
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
