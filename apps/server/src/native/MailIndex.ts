// @effect-diagnostics nodeBuiltinImport:off -- Mail's Envelope Index is a local SQLite file opened read only.
import * as NodeSqlite from "node:sqlite";
import * as DateTime from "effect/DateTime";
import type { NativeSnapshot } from "./NativeHost.ts";

/**
 * Read-only view of Mail's Envelope Index. AppleScript costs ~0.1s per property per message,
 * so listing goes through the index and only content and writes go through Mail itself.
 * Message ROWIDs are the same ids Mail's scripting dictionary uses.
 */
export interface MailIndexMailbox {
  readonly url: string;
  readonly accountId: string;
  readonly path: string;
  readonly title: string;
  readonly unread: number;
}

const decodePath = (url: string) => {
  const match = /^(?:imap|ews|pop|local):\/\/([^/]+)\/(.+)$/.exec(url);
  if (!match) return null;
  try {
    return { accountId: match[1]!, path: decodeURIComponent(match[2]!) };
  } catch {
    return null;
  }
};

export const mailboxFromUrl = (url: string): Omit<MailIndexMailbox, "unread"> | null => {
  // local:// mailboxes are Mail's own outbox and send-later queues, not user mail.
  if (url.startsWith("local://")) return null;
  const parts = decodePath(url);
  if (!parts) return null;
  return { url, ...parts, title: parts.path.replace(/^\[Gmail\]\//, "") };
};

export const mailboxes = (db: NodeSqlite.DatabaseSync): ReadonlyArray<MailIndexMailbox> =>
  (
    db.prepare("SELECT url, unread_count FROM mailboxes ORDER BY url").all() as Array<{
      url: string;
      unread_count: number;
    }>
  ).flatMap((row) => {
    const mailbox = mailboxFromUrl(row.url);
    return mailbox ? [{ ...mailbox, unread: row.unread_count }] : [];
  });

type MessageRow = {
  id: number;
  url: string;
  prefix: string | null;
  subject: string | null;
  summary: string | null;
  received: number | null;
  read: number;
  address: string | null;
  comment: string | null;
};
const SELECT = `SELECT m.ROWID AS id, mb.url AS url, m.subject_prefix AS prefix, s.subject AS subject,
  su.summary AS summary, m.date_received AS received, m.read AS read, a.address AS address, a.comment AS comment
  FROM messages m JOIN mailboxes mb ON mb.ROWID = m.mailbox
  LEFT JOIN subjects s ON s.ROWID = m.subject LEFT JOIN summaries su ON su.ROWID = m.summary
  LEFT JOIN addresses a ON a.ROWID = m.sender`;

const person = (address: string | null, comment: string | null) =>
  address === null ? "" : comment ? `${comment} <${address}>` : address;
const iso = (seconds: number | null) =>
  seconds === null
    ? null
    : DateTime.formatIso(DateTime.makeUnsafe(seconds * 1000)).replace(".000Z", "Z");

const snapshot = (db: NodeSqlite.DatabaseSync, row: MessageRow): NativeSnapshot => {
  const recipients = (
    db
      .prepare(
        "SELECT a.address AS address, a.comment AS comment FROM recipients r JOIN addresses a ON a.ROWID = r.address WHERE r.message = ? AND r.type = 0 ORDER BY r.position LIMIT 50",
      )
      .all(row.id) as Array<{ address: string; comment: string }>
  ).map((r) => person(r.address, r.comment).slice(0, 16000));
  const title = `${row.prefix ?? ""}${row.subject ?? ""}`;
  const body = row.summary ?? "";
  return {
    app: "mail",
    id: String(row.id),
    resourceId: row.url,
    title: title.slice(0, 16000),
    body: body.slice(0, 16000),
    start: iso(row.received),
    end: null,
    completed: row.read === 1,
    sender: person(row.address, row.comment).slice(0, 16000),
    recipients,
    modifiedAt: null,
    recurring: false,
    hasAttendees: false,
    contentTruncated: title.length > 16000 || body.length > 16000,
  };
};

export interface MailQuery {
  readonly urls: ReadonlyArray<string>;
  readonly limit: number;
  readonly includeRead: boolean;
  readonly query?: string | undefined;
  readonly start?: string | undefined;
  readonly end?: string | undefined;
}

export const messages = (db: NodeSqlite.DatabaseSync, input: MailQuery) => {
  if (input.urls.length === 0) return { records: [], truncated: false };
  const where = [`mb.url IN (${input.urls.map(() => "?").join(",")})`, "m.deleted = 0"];
  const params: Array<string | number> = [...input.urls];
  if (!input.includeRead) where.push("m.read = 0");
  if (input.start !== undefined) {
    where.push("m.date_received >= ?");
    params.push(Math.floor(Date.parse(input.start) / 1000));
  }
  if (input.end !== undefined) {
    where.push("m.date_received < ?");
    params.push(Math.floor(Date.parse(input.end) / 1000));
  }
  if (input.query !== undefined) {
    // LIKE patterns are escaped so a query is always a literal substring.
    const pattern = `%${input.query.replace(/[\\%_]/g, (c) => `\\${c}`)}%`;
    where.push(
      "(s.subject LIKE ? ESCAPE '\\' OR a.address LIKE ? ESCAPE '\\' OR a.comment LIKE ? ESCAPE '\\')",
    );
    params.push(pattern, pattern, pattern);
  }
  const rows = db
    .prepare(
      `${SELECT} WHERE ${where.join(" AND ")} ORDER BY m.date_received DESC, m.ROWID DESC LIMIT ?`,
    )
    .all(...params, input.limit + 1) as Array<MessageRow>;
  return {
    records: rows.slice(0, input.limit).map((row) => snapshot(db, row)),
    truncated: rows.length > input.limit,
  };
};

export const message = (db: NodeSqlite.DatabaseSync, id: string): NativeSnapshot | undefined => {
  if (!/^\d{1,18}$/.test(id)) return undefined;
  const row = db.prepare(`${SELECT} WHERE m.ROWID = ? AND m.deleted = 0`).get(Number(id)) as
    | MessageRow
    | undefined;
  return row === undefined ? undefined : snapshot(db, row);
};

export const highWater = (db: NodeSqlite.DatabaseSync) =>
  (db.prepare("SELECT COALESCE(MAX(ROWID), 0) AS id FROM messages").get() as { id: number }).id;

/**
 * Drafts saved after `afterId` in one account with this exact subject. IMAP accounts re-file a
 * new draft under a fresh id once uploaded, so callers wait for a single surviving row.
 */
export const drafts = (
  db: NodeSqlite.DatabaseSync,
  input: { readonly accountId: string; readonly subject: string; readonly afterId: number },
) =>
  (
    db
      .prepare(
        `${SELECT} WHERE m.ROWID > ? AND m.deleted = 0 AND s.subject = ? AND COALESCE(m.subject_prefix, '') = ''
         AND mb.url LIKE ? ESCAPE '\\' ORDER BY m.ROWID DESC LIMIT 5`,
      )
      .all(
        input.afterId,
        input.subject,
        `%://${input.accountId.replace(/[\\%_]/g, (c) => `\\${c}`)}/%Drafts`,
      ) as Array<MessageRow>
  ).map((row) => snapshot(db, row));

/** Newest `V<n>` store wins; Mail migrates in place and leaves no older live index. */
export const indexPath = (mailRoot: string, entries: ReadonlyArray<string>) => {
  const version = entries
    .filter((entry) => /^V\d+$/.test(entry))
    .sort((a, b) => Number(b.slice(1)) - Number(a.slice(1)))[0];
  return version === undefined ? null : `${mailRoot}/${version}/MailData/Envelope Index`;
};

/** Mail pads saved draft text with blank lines and trailing spaces. */
export const normalizeText = (text: string) =>
  text
    .replace(/\r\n?/g, "\n")
    .split("\n")
    .map((line) => line.trimEnd())
    .join("\n")
    .trim();

/** Bare address from Mail's "Name <address>" form. */
export const address = (value: string) => /<([^<>]+)>$/.exec(value)?.[1] ?? value;

export const open = (path: string) => new NodeSqlite.DatabaseSync(path, { readOnly: true });
