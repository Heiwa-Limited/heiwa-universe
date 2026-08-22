import type { AppState } from "./app";

/**
 * Legacy runtime event socket.
 *
 * Predates the operator stream and still carries two signals the operator
 * stream does not: approval-queue changes and goal updates. Everything else
 * in the shell reaches the runtime through Tauri commands; this is the one
 * raw socket, kept because dropping it would silently stop the inbox and
 * approval counts from refreshing. It folds into the connector plane at L3.
 */
export function connectLegacyEvents(
  app: AppState,
  options: { url?: string; reconnectMs?: number } = {},
): () => void {
  const reconnectMs = options.reconnectMs ?? 3000;
  let socket: WebSocket | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let closed = false;

  const endpoint = (): string => {
    if (options.url) return options.url;
    const secure = location.protocol === "https:";
    const port = location.port || (secure ? "443" : "7474");
    return `${secure ? "wss:" : "ws:"}//127.0.0.1:${port}/ws/v1/events`;
  };

  const connect = (): void => {
    if (closed) return;
    try {
      socket = new WebSocket(endpoint());
    } catch {
      timer = setTimeout(connect, reconnectMs);
      return;
    }

    socket.onmessage = (message) => {
      try {
        const event = (JSON.parse(message.data) as { event?: string }).event;
        if (event === "dispatch_request_appeared" || event === "dispatch_request_decided") {
          void Promise.all([app.runtime.loadInbox(), app.runtime.loadApprovals()]);
        }
        if (event === "goal_updated") {
          void app.runtime.loadHealth();
        }
      } catch {
        /* malformed frame: ignore */
      }
    };

    socket.onclose = () => {
      if (!closed) timer = setTimeout(connect, reconnectMs);
    };
  };

  connect();

  return () => {
    closed = true;
    if (timer) clearTimeout(timer);
    socket?.close();
  };
}
