import { describe, expect, it } from "vitest";

/**
 * L-009: `api_post` on the Rust side (`src-tauri/src/proxy.rs`) enforces an
 * explicit allowlist of POST endpoints instead of passing every
 * `/api/v1/*` path through with the machine credential. This test is the
 * other half of that guarantee: it scans the frontend for every literal
 * path a `post`/`apiPost` call actually uses and fails if either side has an
 * entry the other does not, so the two allowlists cannot silently drift
 * apart.
 *
 * Keep `ALLOWED_POST_SHAPES` below in sync with `ALLOWED_POST_EXACT` and
 * `ALLOWED_POST_ID_PATTERNS` in `src-tauri/src/proxy.rs`.
 *
 * A dynamic path segment (a `${...}` hole in a template literal) is
 * normalized to the placeholder `:id:` before comparison. That is
 * deliberately looser than the Rust allowlist, which additionally requires
 * an id segment to be non-empty and slash-free, and — for the approve/deny
 * route specifically — requires the literal suffix to be exactly "approve"
 * or "deny" rather than any id-shaped value. This scanner only proves the
 * *shape* the UI calls is allowlisted; it is not a substitute for the
 * Rust-side unit tests on `post_path_allowed`.
 */
const ALLOWED_POST_SHAPES = [
  "/api/v1/agents/dispatch",
  "/api/v1/calendar/sync",
  "/api/v1/calendar/read",
  "/api/v1/calendar/holds",
  "/api/v1/connectors/apple_calendar/connect",
  "/api/v1/connectors/apple_calendar/disconnect",
  "/api/v1/operator/threads",
  "/api/v1/operator/projects",
  "/api/v1/approvals/:id:/:id:",
  "/api/v1/operator/threads/:id:/metadata",
  "/api/v1/operator/threads/:id:/turns",
  "/api/v1/operator/projects/:id:/metadata",
] as const;

/**
 * Matches a call to `post(...)` or `apiPost(...)` — with or without a
 * `<Generic>` type argument — capturing the first argument when it is a
 * plain string or a template literal.
 *
 * A call whose first argument is a bare variable (no literal at all) is
 * invisible to this static scan. Every real POST call site in this codebase
 * uses a literal or template literal today (verified by inspection during
 * L-009: `src/runtime.ts`, `src/state/runtime.ts`, `src/state/sessions.ts`,
 * `src/operator/client.ts`); this is a documented limitation, not a silent
 * gap in current coverage.
 */
const POST_CALL =
  /\b(?:apiPost|post)\s*(?:<[^>()]*>)?\s*\(\s*(`[^`]*`|"[^"]*"|'[^']*')/g;

// Raw source text of every non-test module, read through Vite's glob import
// rather than Node's `fs` so this test needs no extra type declarations
// beyond what the rest of the Vite/Vitest toolchain already provides.
const sourceFiles = import.meta.glob(
  ["/src/**/*.{ts,tsx}", "!/src/**/*.test.{ts,tsx}"],
  { eager: true, query: "?raw", import: "default" },
) as Record<string, string>;

/** Collapses every `${...}` hole in a template literal to one placeholder:
 * this scan checks path *shape*, not the value an interpolation produces. */
function toShape(rawLiteral: string): string {
  const quote = rawLiteral[0];
  const inner = rawLiteral.slice(1, -1);
  return quote === "`" ? inner.replace(/\$\{[^}]*\}/g, ":id:") : inner;
}

/** Path shape -> the files it was found in (for a readable failure message). */
function findUsedPostShapes(): Map<string, string[]> {
  const usages = new Map<string, string[]>();
  for (const [file, text] of Object.entries(sourceFiles)) {
    for (const match of text.matchAll(POST_CALL)) {
      const shape = toShape(match[1]);
      if (!shape.startsWith("/")) continue; // a post()/apiPost() call whose argument is not a path
      const files = usages.get(shape) ?? [];
      files.push(file);
      usages.set(shape, files);
    }
  }
  return usages;
}

describe("api_post allowlist matches what the UI actually calls", () => {
  it("has no used path outside the allowlist, and no allowlist entry that goes unused", () => {
    const used = findUsedPostShapes();
    const allowed = new Set<string>(ALLOWED_POST_SHAPES);

    // Fold the "where" into each entry so a failure is readable from the
    // assertion diff alone, without a separate thrown-error message.
    const notAllowlisted = [...used.keys()]
      .filter((shape) => !allowed.has(shape))
      .map((shape) => `${shape} (used in ${used.get(shape)?.join(", ")})`);
    const unused = ALLOWED_POST_SHAPES.filter((shape) => !used.has(shape));

    expect(
      notAllowlisted,
      "path(s) used by the UI are missing from ALLOWED_POST_SHAPES here, and must also be " +
        "added to ALLOWED_POST_EXACT/ALLOWED_POST_ID_PATTERNS in src-tauri/src/proxy.rs",
    ).toEqual([]);
    expect(
      unused,
      "ALLOWED_POST_SHAPES entries are not called anywhere in src/**/*.{ts,tsx} — remove " +
        "them here and from proxy.rs, or the allowlist is wider than the UI needs",
    ).toEqual([]);
  });
});
