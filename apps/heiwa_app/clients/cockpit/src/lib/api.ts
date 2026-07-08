export type ApiError = { status: number; message: string };

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, {
    ...init,
    headers: { "Content-Type": "application/json", ...(init?.headers ?? {}) },
  });
  if (!res.ok) {
    const message = await res.text().catch(() => res.statusText);
    throw { status: res.status, message } satisfies ApiError;
  }
  return res.json() as Promise<T>;
}

export const api = {
  get: <T>(path: string) => request<T>(path),
  post: <T>(path: string, body: unknown) =>
    request<T>(path, { method: "POST", body: JSON.stringify(body) }),
};

export function openWs(path: string): WebSocket {
  const url = new URL(path, window.location.origin);
  url.protocol = url.protocol.replace("http", "ws");
  return new WebSocket(url);
}

export interface SseEvent {
  event: string;
  data: unknown;
}

/**
 * POST a JSON body and consume the text/event-stream response incrementally.
 * EventSource only supports GET, so this parses SSE frames off a fetch body.
 * Returns an abort handle; resolves when the stream ends.
 */
export async function postSse(
  path: string,
  body: unknown,
  onEvent: (event: SseEvent) => void,
  signal?: AbortSignal,
): Promise<void> {
  const res = await fetch(path, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
    signal: signal ?? null,
  });
  if (!res.ok || !res.body) {
    const message = await res.text().catch(() => res.statusText);
    throw { status: res.status, message } satisfies ApiError;
  }

  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";

  const dispatch = (frame: string): void => {
    let eventName = "message";
    const dataLines: string[] = [];
    for (const line of frame.split("\n")) {
      if (line.startsWith("event:")) eventName = line.slice(6).trim();
      else if (line.startsWith("data:")) dataLines.push(line.slice(5).trim());
    }
    if (dataLines.length === 0) return;
    const raw = dataLines.join("\n");
    let data: unknown = raw;
    try {
      data = JSON.parse(raw);
    } catch {
      // keep raw string for non-JSON payloads
    }
    onEvent({ event: eventName, data });
  };

  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
    let sep = buffer.indexOf("\n\n");
    while (sep !== -1) {
      dispatch(buffer.slice(0, sep));
      buffer = buffer.slice(sep + 2);
      sep = buffer.indexOf("\n\n");
    }
  }
  if (buffer.trim().length > 0) dispatch(buffer);
}
