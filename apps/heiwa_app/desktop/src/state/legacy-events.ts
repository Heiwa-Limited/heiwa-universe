import type { AppState } from "./app";

/**
 * Legacy runtime event refresh.
 *
 * Carries two signals the operator stream does not: approval-queue changes
 * (`dispatch_request_appeared` / `dispatch_request_decided`, which also touch
 * the inbox) and goal updates. This used to be a raw `WebSocket` straight to
 * `/ws/v1/events`, the one place the shell talked to the runtime outside a
 * signed Tauri command.
 *
 * The runtime now requires authentication on every `/ws/` path (L-007). A
 * plain browser `WebSocket` cannot carry the runtime's signed-request
 * headers, and this window's own page origin is the bundled Tauri asset
 * origin, not `http://127.0.0.1:<port>` — so it never holds that origin's
 * session cookie either (and `SameSite=Strict` would block the cookie from
 * attaching cross-site even if it somehow did). There is no way for a raw
 * socket here to authenticate, so this polls the same signed Tauri commands
 * (`loadInbox`, `loadApprovals`, `loadHealth`) the message handler already
 * called once notified, rather than adding a new native command surface —
 * L-009 is mid-flight on that command manifest and a new command would break
 * its drift test. Mirrors the visible/hidden/focus refresh idiom in
 * `app.tsx`'s surface loop. Folds into the connector plane at L3.
 */
export function connectLegacyEvents(
  app: AppState,
  options: { intervalMs?: number } = {},
): () => void {
  const intervalMs = options.intervalMs ?? 5000;
  let running = false;
  const visible = () => document.visibilityState !== "hidden";

  const run = (): void => {
    if (running) return;
    running = true;
    Promise.all([
      app.runtime.loadInbox(),
      app.runtime.loadApprovals(),
      app.runtime.loadHealth(),
    ])
      .catch(() => undefined) // each loader already keeps its last-known
      // state on failure; this only stops a poll tick from becoming an
      // unhandled rejection or an error storm.
      .finally(() => {
        running = false;
      });
  };

  const onReturn = (): void => {
    if (visible()) run();
  };

  run();
  window.addEventListener("focus", onReturn);
  document.addEventListener("visibilitychange", onReturn);
  const timer = setInterval(() => visible() && run(), intervalMs);

  return () => {
    window.removeEventListener("focus", onReturn);
    document.removeEventListener("visibilitychange", onReturn);
    clearInterval(timer);
  };
}
