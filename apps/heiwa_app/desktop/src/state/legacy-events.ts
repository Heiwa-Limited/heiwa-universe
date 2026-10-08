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
 * the message handler already called once notified, rather than adding a new
 * native command surface — L-009 is mid-flight on that command manifest and
 * a new command would break its drift test. Mirrors the visible/hidden/focus
 * refresh idiom in `app.tsx`'s surface loop. Folds into the connector plane
 * at L3.
 *
 * `loadHealth` is on its own, much slower cadence than `loadInbox` /
 * `loadApprovals`: the runtime snapshot it reads is comparatively expensive
 * (mail, approvals, workers, and hooks state, a keep-awake check, and a
 * self-probe of the port), and nothing else in the shell polls it — only
 * `main.tsx` calls it once, at boot. Polling it on the same 5s cadence as
 * the cheap inbox/approval reads would be new sustained load with no
 * corresponding need for that freshness.
 */
export function connectLegacyEvents(
  app: AppState,
  options: { intervalMs?: number; healthIntervalMs?: number } = {},
): () => void {
  const intervalMs = options.intervalMs ?? 5000;
  const healthIntervalMs = options.healthIntervalMs ?? 30000;
  let running = false;
  let healthRunning = false;
  let lastHealthAttempt = 0;
  const visible = () => document.visibilityState !== "hidden";

  const run = (): void => {
    if (running) return;
    running = true;
    Promise.all([app.runtime.loadInbox(), app.runtime.loadApprovals()])
      .catch(() => undefined) // each loader already keeps its last-known
      // state on failure; this only stops a poll tick from becoming an
      // unhandled rejection or an error storm.
      .finally(() => {
        running = false;
      });
  };

  // Gated by elapsed time since the last attempt (successful or not), not
  // just "in flight", so a rapid string of ticks/focus bounces cannot queue
  // up calls the moment the current one finishes.
  const maybeRefreshHealth = (): void => {
    if (healthRunning) return;
    const now = Date.now();
    if (now - lastHealthAttempt < healthIntervalMs) return;
    lastHealthAttempt = now;
    healthRunning = true;
    app.runtime
      .loadHealth()
      .catch(() => undefined)
      .finally(() => {
        healthRunning = false;
      });
  };

  const tick = (): void => {
    run();
    maybeRefreshHealth();
  };

  const onReturn = (): void => {
    if (visible()) tick();
  };

  tick();
  window.addEventListener("focus", onReturn);
  document.addEventListener("visibilitychange", onReturn);
  const timer = setInterval(() => visible() && tick(), intervalMs);

  return () => {
    window.removeEventListener("focus", onReturn);
    document.removeEventListener("visibilitychange", onReturn);
    clearInterval(timer);
  };
}
