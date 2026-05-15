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
