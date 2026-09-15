/**
 * Presentation helpers.
 *
 * Note the absence of an `esc()` helper: the pre-Solid shell built HTML with
 * template strings and had to escape every interpolation by hand, so a single
 * missed call was an injection path for runtime-supplied text. Solid's JSX
 * creates text nodes, so escaping is structural.
 */

export function timeFmt(ts?: number): string {
  if (!ts || Number.isNaN(ts)) return "";
  return new Date(ts).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

export function shortDate(raw?: string): string {
  if (!raw) return "";
  const parsed = parseLocalDate(raw) ?? new Date(raw);
  if (Number.isNaN(parsed.getTime())) return raw;
  return parsed.toLocaleDateString([], { month: "short", day: "numeric" });
}

/**
 * A `YYYY-MM-DD` calendar day as local midnight, or null for anything else.
 *
 * `new Date("2026-09-14")` is UTC midnight by specification, which is the
 * evening of the 13th anywhere west of UTC. Calendar days are local, so they
 * are built from their parts instead.
 */
export function parseLocalDate(raw: string): Date | null {
  const match = raw.match(/^(\d{4})-(\d{2})-(\d{2})$/);
  if (!match) return null;
  const [year, month, day] = [Number(match[1]), Number(match[2]), Number(match[3])];
  const date = new Date(year, month - 1, day);
  // Reject rollovers such as 2026-02-30 rather than silently moving the day.
  return date.getFullYear() === year && date.getMonth() === month - 1 && date.getDate() === day
    ? date
    : null;
}

/**
 * A date as `YYYY-MM-DD` in the machine's own timezone.
 *
 * Deliberately not `toISOString().slice(0, 10)`: that answers in UTC, which
 * is a different calendar day from local for part of every day — evening west
 * of UTC, morning east of it. Local calendar events carry the user's local
 * day, so any comparison against them has to be made in the same frame or
 * today's events disappear near the boundary.
 */
export function localIsoDate(when: Date = new Date()): string {
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${when.getFullYear()}-${pad(when.getMonth() + 1)}-${pad(when.getDate())}`;
}

/** Collapse any user's home prefix to `~` for display. */
export function shortenPath(raw: string): string {
  return raw.replace(/^\/(?:Users|home)\/[^/]+/, "~");
}

/** Normalize an arbitrary status string into a CSS-safe modifier class. */
export function cssToken(raw: string): string {
  return raw.toLowerCase().replace(/[^a-z0-9_-]+/g, "-");
}
