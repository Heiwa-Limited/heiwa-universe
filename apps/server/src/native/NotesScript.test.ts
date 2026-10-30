// @effect-diagnostics nodeBuiltinImport:off -- run the shipped data script against a minimal Apple Notes dictionary fixture.
import { runInNewContext } from "node:vm";
import * as DateTime from "effect/DateTime";
import { assert, it } from "@effect/vitest";
import { NOTES_SCRIPT } from "./NotesScript.ts";

it("updates the independent title field used by IMAP Notes accounts", () => {
  let title = "Before",
    body = "Original body",
    html = "<div>Before</div><div>Original body</div>";
  const folder = {
    id: () => "folder",
    name: () => "Notes",
    shared: () => false,
    notes: () => [note],
  };
  const note = new Proxy(
    {},
    {
      get: (_target, key) =>
        ({
          name: () => title,
          body: () => html,
          plaintext: () => `${title}\n${body}`,
          id: () => "note",
          container: () => folder,
          modificationDate: () => DateTime.toDateUtc(DateTime.makeUnsafe("2026-10-30T10:00:00Z")),
          attachments: () => [],
          shared: () => false,
        })[key as string],
      set: (_target, key, value) => {
        if (key === "name") title = value;
        if (key === "body") {
          html = value;
          body = "Replacement body";
        }
        return true;
      },
    },
  );
  const expected = {
    app: "notes",
    id: "note",
    resourceId: "folder",
    title,
    body,
    bodyHtml: html,
    start: null,
    end: null,
    completed: false,
    modifiedAt: "2026-10-30T10:00:00.000Z",
    recurring: false,
    hasAttendees: false,
    contentTruncated: false,
    hasAttachments: false,
    locked: false,
  };
  const request = {
    app: "notes",
    operation: "update",
    resourceIds: ["folder"],
    resourceId: "folder",
    id: "note",
    expected,
    title: "After",
    body: "Replacement body",
  };
  const reply = JSON.parse(
    runInNewContext(`${NOTES_SCRIPT}\nrun()`, {
      ObjC: { import: () => undefined, unwrap: (x: unknown) => x },
      $: {
        NSFileHandle: { fileHandleWithStandardInput: { readDataToEndOfFile: "fixture" } },
        NSString: { alloc: { initWithDataEncoding: () => JSON.stringify(request) } },
        NSUTF8StringEncoding: 4,
      },
      Application: () => ({ folders: () => [folder] }),
    }),
  );
  assert.equal(reply.record?.title, "After");
  assert.equal(reply.record?.body, "Replacement body");
});
