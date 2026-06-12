import { invoke } from "@tauri-apps/api/core";

export type ApiErrorPayload =
  | { kind: "Offline"; detail: string }
  | { kind: "Http"; detail: { status: number; body: string } }
  | { kind: "Decode"; detail: string }
  | { kind: "InvalidPath"; detail: string };

export type RuntimeHealth = {
  reachable: boolean;
  snapshot?: RuntimeSnapshotEnvelope | null;
  error?: ApiErrorPayload | null;
};

export type RuntimeSnapshotEnvelope = {
  ok?: boolean;
  data?: {
    runtime_version?: string;
    started_at?: string;
    status?: string;
    notes?: string[];
    [key: string]: unknown;
  };
  [key: string]: unknown;
};

export async function runtimeHealth(): Promise<RuntimeHealth> {
  return invoke<RuntimeHealth>("runtime_health");
}

export async function apiGet<T>(path: string): Promise<T> {
  return invoke<T>("api_get", { path });
}

export async function apiPost<T>(path: string, body: unknown): Promise<T> {
  return invoke<T>("api_post", { path, body });
}

export function runtimeVersion(health: RuntimeHealth | null): string {
  return health?.snapshot?.data?.runtime_version ?? "unknown";
}

export function runtimeStatus(health: RuntimeHealth | null): string {
  if (!health) return "checking";
  if (!health.reachable) return "offline";
  return health.snapshot?.data?.status ?? "ok";
}
