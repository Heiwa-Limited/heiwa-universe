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
  const parsed = new Date(raw);
  if (Number.isNaN(parsed.getTime())) return raw;
  return parsed.toLocaleDateString([], { month: "short", day: "numeric" });
}

/** Collapse any user's home prefix to `~` for display. */
export function shortenPath(raw: string): string {
  return raw.replace(/^\/(?:Users|home)\/[^/]+/, "~");
}

/** Normalize an arbitrary status string into a CSS-safe modifier class. */
export function cssToken(raw: string): string {
  return raw.toLowerCase().replace(/[^a-z0-9_-]+/g, "-");
}
