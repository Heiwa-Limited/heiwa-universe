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
  role: "chat" | "build" | "research" | "audit" | string;
  provider: string | null;
  model: string | null;
  rate_group?: string;
  source: "drex_live" | "drex_no_match" | "no_model_tiers" | string;
  fallbacks: string[];
  offline_capable: boolean;
}

export interface RoutePreview {
  mode: "deterministic" | "local_model" | "remote_model" | "unavailable";
  response?: string;
  error?: string;
  intent?: string;
  provider?: string;
  model?: string;
  provider_model?: string;
  rate_group?: string;
  privacy?: "standard" | "sovereign" | string;
  metadata?: unknown;
  quota: string[];
}

export interface CalendarLaneLive {
  id: string;
  name: string;
  status: "ready" | "connected" | "staged" | "needs_auth" | "planned" | string;
  sync: string;
  write: string;
  evidence: string;
}

export interface CalendarHold {
  id: string;
  title: string;
  date: string;
  start: string | null;
  end: string | null;
  kind: "focus" | "travel" | "soft" | string;
  status: "draft" | "committed" | string;
  note: string | null;
  source: string;
  created_at: string;
  external_promotion: string;
}

export interface CalendarMoment {
  time: string;
  title: string;
  source: string;
  pressure: "fixed" | "soft" | "draft" | string;
  detail: string;
}

export interface CalendarSummary {
  command: string;
  date: string;
  timezone: string;
  lanes: CalendarLaneLive[];
  holds: CalendarHold[];
  today: CalendarMoment[];
  counts: {
    holds_total: number;
    holds_today: number;
    moments_today: number;
  };
}

export interface MailLaneLive {
  id: string;
  name: string;
  status: "connected" | "staged" | "needs_auth" | "metadata" | "planned" | string;
  read: string;
  reply: string;
  guardrail: string;
}

export interface MailPriorityRow {
  account: string | null;
  mailbox: string | null;
  sender: string | null;
  subject: string | null;
  date: string | null;
  unread: boolean | null;
  score: number;
  action: "draft" | "report" | "digest" | string;
}

export interface MailSummary {
  command: string;
  policy: string;
  lanes: MailLaneLive[];
  accounts: string[];
  snapshot: { path: string; present: boolean; scanned: number };
  priority: MailPriorityRow[];
  counts: {
    priority: number;
    unread_in_priority: number;
    accounts: number;
  };
}

export interface Connector {
  id: string;
  kind: "provider" | "calendar" | "mail" | string;
  display_name: string;
  status:
    | "connected"
    | "staged"
    | "needs_auth"
    | "metadata"
    | "planned"
    | "error"
    | string;
  auth_kind: string;
  rate_group?: string | null;
  scopes?: string | null;
  detail: string;
  next_action: string | null;
}

export interface ConnectorsSummary {
  connectors: Connector[];
  counts: Record<string, number>;
  policy: string[];
}

export interface ReplTrace {
  intent: string;
  mode: string;
  provider: string;
  model: string;
  rate_group?: string;
  privacy?: string;
  cost_usd?: number;
  compression?: {
    applied: boolean;
    reason: string;
    ratio: number;
    estimated_usd_saved: number;
  } | null;
  summary?: string;
}

export interface ReplRouteEvent {
  mode: string;
  intent?: string;
  provider?: string;
  model?: string;
  provider_model?: string;
  rate_group?: string;
  privacy?: string;
  request_id?: string;
}

export interface FileTreeEntry {
  name: string;
  path: string;
  kind: "directory" | "file" | "other" | string;
  size_bytes: number | null;
  modified_unix: number | null;
  hidden: boolean;
}

export interface FileTreePayload {
  command: string;
  root: string;
  path: string;
  parent: string | null;
  entries: FileTreeEntry[];
  truncated: boolean;
  limit: number;
  policy: string;
}

export interface FilePreviewPayload {
  command: string;
  path: string;
  name?: string;
  extension?: string | null;
  kind: "directory" | "file" | string;
  size_bytes: number | null;
  modified_unix?: number | null;
  truncated: boolean;
  limit?: number;
  binary?: boolean;
  content: string | null;
  message?: string;
  policy?: string;
}

export interface BrowserProbePayload {
  command: string;
  url: string;
  host: string;
  mode: string;
  policy: string;
  notes: string[];
}

export interface ResourceSnapshotPayload {
  snapshot: {
    cpu_count: number;
    load_1m: number;
    free_memory_bytes: number;
    battery_percent: number | null;
    on_battery: boolean;
    thermal_pressure: string;
  };
  policy: Record<string, unknown>;
  admissions: Record<string, boolean>;
  sources: Record<string, string>;
  notes: string[];
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

export interface SourceRef {
  source_id: string;
  source_type: "dispatch_result" | "event_log" | string;
  label: string;
  uri: string;
}

export interface ReceiptRef {
  kind: string;
  ref: string;
}

export interface InboxItem {
  item_id: string;
  kind: "dispatch_result" | "event" | string;
  plane: "intake" | "execution" | "evidence" | string;
  priority: "low" | "normal" | "high" | string;
  pinned: boolean;
  status: string;
  title: string;
  summary: string;
  occurred_at: string;
  source: SourceRef;
  subject_ref: string;
  receipt_refs: ReceiptRef[];
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

export type AutomationTriggerConfig =
  | {
      type: "cron";
      schedule: string;
      timezone: string | null;
    }
  | {
      type: "file_watch";
      paths: string[];
      events: string[];
      pattern: string | null;
      debounce_ms: number | null;
    }
  | Record<string, unknown>;

export interface Automation {
  id: string;
  name: string;
  description: string | null;
  prompt: string;
  trigger_config: AutomationTriggerConfig | null;
  status: "active" | "paused" | "disabled" | string;
  max_iterations: number;
  max_executions_per_day: number | null;
  max_executions_per_hour: number | null;
  last_executed_at: string | null;
  next_scheduled_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface AutomationExecution {
  id: string;
  automation_id: string;
  status:
    | "pending"
    | "running"
    | "awaiting_confirmation"
    | "completed"
    | "failed"
    | "cancelled"
    | string;
  created_at: string;
  started_at: string | null;
  completed_at: string | null;
  error_message: string | null;
}

export interface AutomationsSummary {
  command: string;
  state_dir: string;
  db_path?: string;
  automation_count: number;
  active_count: number;
  scheduler: {
    active_cron: number;
    active_file_watch: number;
    next_scheduled_at: string | null;
  };
  automations: Automation[];
  recent_executions: AutomationExecution[];
  next?: string[];
  error?: string;
}

export interface Receipt {
  lane: string;
  receipt_id: string;
  kind: string;
  event: string | null;
  created_at: string;
  path: string;
  relative_path: string;
  size_bytes: number | null;
  modified_unix: number | null;
  parse_error: string | null;
  data: Record<string, unknown>;
}

export interface ReceiptsSummary {
  command: string;
  state_dir: string;
  counts: Record<string, number>;
  receipts: Receipt[];
  truncated: boolean;
  limit: number;
  next?: string[];
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

export interface Appointment {
  kind: string;
  date: string;
  note: string;
}

export interface StaleFact {
  label: string;
  source: string;
  age_days: number;
  sla_days: number;
}

export interface PendingApproval {
  id: string;
  action: string;
  target: string;
  risk: string;
  requested_at: string | null;
}

export interface TodaySnapshot {
  command: string;
  date: string;
  timezone: string;
  day_type: "off" | "work" | "unknown" | string;
  work_shifts: string[];
  appointments: Appointment[];
  proof_target: string | null;
  scorecard_notes: string | null;
  stale_facts: StaleFact[];
  pending_approvals: PendingApproval[];
  runtime: { stdb_mode: string };
  next: string[];
  calendar?: { holds_today: number; holds: CalendarHold[] };
  mail?: { priority_count: number; draft_tier: number; top: MailPriorityRow[] };
}

export interface FreshnessSource {
  group: string;
  label: string;
  path: string;
  present: boolean;
  modified_unix: number | null;
  age_days: number | null;
  sla_days: number;
  stale: boolean;
}

export interface FreshnessReport {
  command: string;
  date: string;
  stale_sources: number;
  sources: FreshnessSource[];
}

export interface ApprovalsSummary {
  pending_count: number;
  pending: PendingApproval[];
  requests_dir: string;
  decisions_dir: string;
}
