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
    providers?: ProviderSnapshot[];
    resource?: ResourceSnapshot;
    workers?: WorkersSnapshot;
    approvals?: ApprovalsSnapshot;
    [key: string]: unknown;
  };
  [key: string]: unknown;
};

export type ProviderSnapshot = {
  provider_id: string;
  display_name: string;
  status: string;
  auth_kind: string;
  default_model: string | null;
  supported_lanes: string[];
  last_error: string | null;
  last_validated_at: string | null;
};

export type ResourceSnapshot = {
  snapshot?: {
    cpu_count?: number;
    free_memory_bytes?: number;
    load_1m?: number;
    on_battery?: boolean;
    thermal_pressure?: string;
  };
  admissions?: Record<string, { decision: string }>;
};

export type WorkersSnapshot = {
  total?: number;
  live?: number;
  stale?: number;
};

export type ApprovalsSnapshot = {
  pending?: number;
  decided?: number;
};

export type AgentRow = {
  agent_id?: string;
  provider?: string;
  model?: string;
  status?: string;
  lane?: string;
  last_used?: string;
  [key: string]: unknown;
};

export type SubagentDispatchRequest = {
  task: string;
  provider?: string;
  model?: string;
  lane?: "local" | "cloud" | "auto";
  context?: string;
  approval_policy?: "auto" | "ask" | "deny";
};

export type SubagentDispatchResponse = {
  ok?: boolean;
  data?: {
    agent_id?: string;
    provider?: string;
    model?: string;
    status?: string;
    response?: string;
    trace?: Record<string, unknown>;
    error?: string;
  };
};

export type OllamaModel = {
  name: string;
  size?: number;
  modified?: string;
  parameter_size?: string;
  quantization_level?: string;
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

export async function dispatchSubagent(req: SubagentDispatchRequest): Promise<SubagentDispatchResponse> {
  return apiPost<SubagentDispatchResponse>("/api/v1/agents/dispatch", req);
}

export async function listOllamaModels(): Promise<{ models: OllamaModel[] }> {
  return apiGet<{ models: OllamaModel[] }>("/api/v1/providers/ollama/models");
}

export function runtimeVersion(health: RuntimeHealth | null): string {
  return health?.snapshot?.data?.runtime_version ?? "unknown";
}

export function runtimeStatus(health: RuntimeHealth | null): string {
  if (!health) return "checking";
  if (!health.reachable) return "offline";
  return health.snapshot?.data?.status ?? "ok";
}

export function providersFromSnapshot(health: RuntimeHealth | null): ProviderSnapshot[] {
  return health?.snapshot?.data?.providers ?? [];
}

export function resourceFromSnapshot(health: RuntimeHealth | null): ResourceSnapshot | null {
  return health?.snapshot?.data?.resource ?? null;
}

export function workersFromSnapshot(health: RuntimeHealth | null): WorkersSnapshot | null {
  return health?.snapshot?.data?.workers ?? null;
}

export function approvalsFromSnapshot(health: RuntimeHealth | null): ApprovalsSnapshot | null {
  return health?.snapshot?.data?.approvals ?? null;
}
