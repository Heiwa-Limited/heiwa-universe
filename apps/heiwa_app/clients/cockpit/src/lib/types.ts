export interface Envelope<T> {
  ok: true;
  data: T;
}

export interface ErrorEnvelope {
  ok: false;
  error: { code: string; message: string; details?: Record<string, unknown> };
}

export type ApiResult<T> = Envelope<T> | ErrorEnvelope;

export interface Session {
  operator_id: string;
  hostname: string;
  runtime_version: string;
  channel: "stable" | "nightly";
  default_route_role: string;
  app_url: string;
}

export interface ProviderLive {
  provider_id: string;
  display_name: string;
  auth_kind: "oauth_cli" | "api_key" | "local" | "subscription";
  status: "connected" | "degraded" | "unlinked" | "error";
  rate_group: string | null;
  default_model: string | null;
  last_validated_at: string | null;
  last_error: string | null;
  supported_lanes: string[];
}

export interface Route {
  role: "code" | "chat" | "reason" | "review" | string;
  provider: string;
  model: string;
  source: "default" | "override" | "env";
  fallbacks: string[];
  offline_capable: boolean;
}

export interface Mission {
  mission_id: string;
  prompt: string;
  status: "running" | "paused" | "done" | "failed" | "canceled" | string;
  intent_class: string | null;
  target_tool: string | null;
  target_model: string | null;
  summary: string | null;
  updated_at: string;
}

export interface Approval {
  approval_id: string;
  mission_id: string;
  risk_level: "low" | "medium" | "high" | "critical" | string;
  summary: string;
  requested_at: string;
  expires_at: string | null;
  requested_by: string;
}

export interface RateGroup {
  group_id: string;
  priority: number;
  status: "healthy" | "throttled" | "exhausted" | "down" | string;
  providers: string[];
  quota_state: Record<string, unknown>;
  notes: string | null;
}

export interface HistorySummary {
  sessions: Array<{
    id: string;
    started_at: string;
    ended_at: string | null;
    mission_count: number;
  }>;
  recent_runs: Array<{
    mission_id: string;
    status: string;
    updated_at: string;
    summary: string | null;
  }>;
  artifacts: Array<{
    id: string;
    kind: string;
    label: string;
    updated_at: string;
  }>;
  cursor: string | null;
}

export interface Trace {
  trace_id: string;
  session_id: string;
  mission_id: string;
  route: { role: string; provider: string; model: string };
  receipts: Array<{ kind: string; ref: string }>;
  artifacts: Array<{ id: string; kind: string; label: string }>;
  started_at: string;
  ended_at: string | null;
}

export interface MemoryEntry {
  entry_id: string;
  scope: "user" | "project" | "session";
  title: string;
  summary: string | null;
  source: string | null;
  updated_at: string;
}

export interface Agent {
  agent_id: string;
  parent_id: string | null;
  status: "spawning" | "running" | "attached" | "exited" | "killed" | string;
  role: string;
  started_at: string;
  last_event_at: string | null;
}

export interface Cron {
  job_id: string;
  name: string;
  schedule: string;
  status: "enabled" | "disabled" | "running" | string;
  last_run_at: string | null;
  next_run_at: string | null;
}

export interface HookCommand {
  name: string | null;
  kind: string | null;
  command: string;
  command_path: string | null;
  command_exists: boolean | null;
  timeout_ms: number | null;
}

export interface HookEvent {
  event: string;
  matcher: string;
  hooks: HookCommand[];
}

export interface HookProvider {
  provider_id: string;
  display_name: string;
  status:
    | "active"
    | "degraded"
    | "unconfigured"
    | "unsupported"
    | "delegated"
    | string;
  config_path: string;
  generated_config_status: string;
  audit_file: string | null;
  events: HookEvent[];
  notes: string[];
}

export interface HookSummary {
  source: string;
  providers: number;
  active: number;
  degraded: number;
  unconfigured: number;
  unsupported: number;
  delegated: number;
  events: number;
  commands: number;
}

export interface CellsCatalogEntry {
  id: string;
  label: string;
  category: string;
  description: string | null;
}

export interface Health {
  status: "ok" | "degraded" | "down";
  runtime_version: string;
  started_at: string;
  notes: string[];
}
