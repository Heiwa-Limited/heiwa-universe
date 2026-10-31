// @effect-diagnostics nodeBuiltinImport:off -- builds a minimal Envelope Index fixture.
import * as NodeSqlite from "node:sqlite";
import { assert, it } from "@effect/vitest";
import * as DateTime from "effect/DateTime";
import * as MailIndex from "./MailIndex.ts";

const fixture = () => {
  const db = new NodeSqlite.DatabaseSync(":memory:");
  db.exec(`
    CREATE TABLE mailboxes (ROWID INTEGER PRIMARY KEY, url TEXT, unread_count INTEGER);
    CREATE TABLE subjects (ROWID INTEGER PRIMARY KEY, subject TEXT);
    CREATE TABLE summaries (ROWID INTEGER PRIMARY KEY, summary TEXT);
    CREATE TABLE addresses (ROWID INTEGER PRIMARY KEY, address TEXT, comment TEXT);
    CREATE TABLE recipients (ROWID INTEGER PRIMARY KEY, message INTEGER, address INTEGER, type INTEGER, position INTEGER);
    CREATE TABLE messages (ROWID INTEGER PRIMARY KEY, sender INTEGER, subject_prefix TEXT, subject INTEGER,
      summary INTEGER, date_received INTEGER, mailbox INTEGER, read INTEGER, deleted INTEGER);
    INSERT INTO mailboxes VALUES (1, 'imap://ACCT/INBOX', 2), (2, 'imap://ACCT/%5BGmail%5D/All%20Mail', 0),
      (3, 'local://LOCAL/Outbox', 0);
    INSERT INTO subjects VALUES (1, 'Invoice 100%_paid'), (2, 'Lunch'), (3, 'Old');
    INSERT INTO summaries VALUES (1, 'Preview');
    INSERT INTO addresses VALUES (1, 'billing@example.com', 'Billing'), (2, 'me@example.com', '');
    INSERT INTO messages VALUES
      (10, 1, NULL, 1, 1, 1793400000, 1, 0, 0),
      (11, 2, 'Re: ', 2, NULL, 1793400100, 1, 0, 0),
      (12, 1, NULL, 3, NULL, 1793300000, 1, 1, 0),
      (13, 1, NULL, 2, NULL, 1793400200, 1, 0, 1);
    INSERT INTO recipients VALUES (1, 10, 2, 0, 0), (2, 10, 1, 1, 0);
  `);
  return db;
};

it("lists user mailboxes with readable titles and skips Mail's local queues", () => {
  assert.deepStrictEqual(
    MailIndex.mailboxes(fixture()).map((row) => [row.title, row.accountId, row.unread]),
    [
      ["All Mail", "ACCT", 0],
      ["INBOX", "ACCT", 2],
    ],
  );
});

it("reads newest unread mail, excludes deleted rows and matches queries literally", () => {
  const db = fixture();
  const unread = MailIndex.messages(db, {
    urls: ["imap://ACCT/INBOX"],
    limit: 1,
    includeRead: false,
  });
  assert.deepStrictEqual(
    unread.records.map((row) => [row.id, row.title, row.sender, row.completed]),
    [["11", "Re: Lunch", "me@example.com", false]],
  );
  assert.isTrue(unread.truncated);
  const all = MailIndex.messages(db, { urls: ["imap://ACCT/INBOX"], limit: 10, includeRead: true });
  assert.deepStrictEqual(
    all.records.map((row) => row.id),
    ["11", "10", "12"],
  );
  const invoice = all.records.find((row) => row.id === "10")!;
  assert.deepStrictEqual(invoice.recipients, ["me@example.com"]);
  assert.equal(invoice.body, "Preview");
  assert.equal(
    invoice.start,
    DateTime.formatIso(DateTime.makeUnsafe(1793400000 * 1000)).replace(".000Z", "Z"),
  );
  // `%` and `_` are literal characters, not wildcards.
  assert.deepStrictEqual(
    MailIndex.messages(db, {
      urls: ["imap://ACCT/INBOX"],
      limit: 10,
      includeRead: true,
      query: "100%_",
    }).records.map((row) => row.id),
    ["10"],
  );
  assert.lengthOf(
    MailIndex.messages(db, {
      urls: ["imap://ACCT/INBOX"],
      limit: 10,
      includeRead: true,
      query: "0_p",
    }).records,
    0,
  );
  assert.isUndefined(MailIndex.message(db, "13"));
  assert.isUndefined(MailIndex.message(db, "1 OR 1=1"));
});

it("finds a saved draft by exact subject in the same account after the high-water mark", () => {
  const db = fixture();
  db.exec(`INSERT INTO mailboxes VALUES (4, 'imap://ACCT/Drafts', 0), (5, 'imap://OTHER/Drafts', 0);
    INSERT INTO subjects VALUES (4, 'Plan');
    INSERT INTO messages VALUES (20, 2, NULL, 4, NULL, 1793400300, 4, 1, 0), (21, 2, NULL, 4, NULL, 1793400300, 5, 1, 0),
      (22, 2, 'Re: ', 4, NULL, 1793400300, 4, 1, 0);`);
  assert.deepStrictEqual(
    MailIndex.drafts(db, { accountId: "ACCT", subject: "Plan", afterId: 13 }).map((row) => row.id),
    ["20"],
  );
  assert.lengthOf(MailIndex.drafts(db, { accountId: "ACCT", subject: "Plan", afterId: 20 }), 0);
  assert.equal(MailIndex.normalizeText("\nBody\r\nline two \n"), "Body\nline two");
});

it("chooses the newest Mail store version", () => {
  assert.equal(
    MailIndex.indexPath("/Mail", ["V9", "V10", "PersistenceInfo.plist"]),
    "/Mail/V10/MailData/Envelope Index",
  );
  assert.isNull(MailIndex.indexPath("/Mail", ["Bundles"]));
});
