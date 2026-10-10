/** Shared shapes for data the runtime serves to more than one surface. */

/**
 * A calendar row as the runtime serves it. `start`/`end` are RFC 3339 instants
 * for synced events, `HH:MM` clock times for local holds, or bare dates for
 * Google all-day events; `surfaces/calendar/model.ts` is the one reader.
 */
export type CalendarEvent = {
  id?: string;
  title?: string;
  start?: string;
  end?: string;
  /** Local day the event starts on. */
  date?: string;
  /** Last local day the event touches, inclusive. */
  end_date?: string;
  all_day?: boolean;
  recurring?: boolean;
  calendar?: string;
  kind?: string;
  status?: string;
  source?: string;
  note?: string;
};

/** Inclusive local-day range of calendar rows to load. */
export type CalendarRange = { from: string; to: string };

/**
 * Freshness of the selected Apple calendars, owned by the runtime.
 *
 * `synced` means this request read EventKit; `fresh` means a recent read was
 * reused. `complete` is false when a read could not see the whole window, in
 * which case nothing unseen was deleted.
 */
export type CalendarSyncStatus = {
  status: "synced" | "fresh" | "not_connected" | "no_selection" | "error" | "never";
  last_read_at?: string | null;
  last_attempt_at?: string | null;
  complete?: boolean | null;
  fetched?: number | null;
  selected_count?: number;
  error?: string | null;
};

export type CalendarResource = {
  id?: string;
  source?: string;
  name: string;
  writable: boolean;
};

export type CalendarResources = {
  source: string;
  status: string;
  calendars: CalendarResource[];
  selected_ids?: string[];
  catalog_revision?: string | null;
  reader_available?: boolean;
  detail?: string | null;
  next_action?: string | null;
};

/** Pending choices are deliberately separate from the runtime's saved selection. */
export type CalendarSelection = { ids: string[]; saving: boolean; error: string | null };

export type PendingApproval = {
  id: string;
  action: string;
  target: string;
  risk: string;
  requested_at?: string | null;
};

export type ApprovalsSummary = {
  pending_count: number;
  pending: PendingApproval[];
  requests_dir: string;
  decisions_dir: string;
};

export type InboxItem = {
  title?: string;
  detail?: string;
  occurred_at?: string;
  kind?: string;
  source?: string;
  [key: string]: unknown;
};

/** Per-surface descriptor shown on Home and the feature windows. */
export type SubApp = {
  id: string;
  title: string;
  server: string;
  state: string;
  skills: string[];
  tools: string[];
  personalization: string[];
};

/**
 * One thing first run still needs, and the action that closes it.
 *
 * Mirrors `heiwa_identity::onboarding` — the Rust projection is the only
 * place readiness is decided, so these are carried across, never recomputed.
 */
export type OnboardingStep = "state_root" | "identity" | "provider";

export type OnboardingGap = {
  step: OnboardingStep;
  detail: string;
  remedy: string;
};

export type OnboardingState = {
  complete: boolean;
  gaps: OnboardingGap[];
  display_name: string | null;
  /** Desktop welcome completion does not imply provider or connector access. */
  workspace?: {
    can_enter: boolean;
    setup_complete: boolean;
    resources: DiscoveredResource[];
    connections?: ProviderConnection[];
    cli_path?: string | null;
  };
};

export type ProviderConnection = {
  account_id: string;
  provider: string;
  channel: string;
  status: "connected" | "disconnected" | "needs_verification" | "verification_failed";
  model_count: number;
  can_manage_key: boolean;
};

export type DiscoveredResource = {
  id: string;
  name: string;
  category: "apple" | "inference";
  app_detected: boolean;
  tools_detected: string[];
  registered_accounts: number;
  detail: string;
  surface: "calendar" | "mail" | "reminders" | null;
  has_guide: boolean;
};

/**
 * One message from the local mail snapshot.
 *
 * Metadata only, by policy: `heiwa mail scan` reads sender, subject, date,
 * and read state from the user's own Mail.app and never touches a body. The
 * snapshot lives under the config root and no part of it leaves the machine.
 */
export type MailMessage = {
  sender: string;
  subject: string;
  unread: boolean;
  account?: string;
  mailbox?: string;
  date?: string;
};

/** Freshness and outcome of the local Apple Mail metadata sync. */
export type MailSyncStatus = {
  status: "scanned" | "fresh" | "no_consent" | "mail_not_running" | "backoff" | "error" | "skipped";
  freshness?: string;
  fetched?: number;
  appended?: number;
  updated?: number;
  removed?: number;
  error?: string | null;
  error_class?: "timeout" | "permission_pending" | "automation_denied" | "backoff" | "failed" | string | null;
  last_scan_at?: string | null;
  last_attempt_at?: string | null;
};

/** A problem one finance source reported during a sync. */
export type FinanceIssue = { source: string; message: string };

export type FinanceAccount = {
  id: string;
  name: string;
  institution: string;
  kind: string;
  number_hint?: string | null;
  value?: number | null;
  cash: number;
  positions: number;
};

export type FinancePosition = {
  symbol: string;
  description?: string | null;
  kind: string;
  currency?: string | null;
  units: number;
  price?: number | null;
  value_base?: number | null;
  weight?: number | null;
  average_cost?: number | null;
  unrealized_gain_base?: number | null;
  unrealized_pct?: number | null;
  accounts: number;
};

export type FinanceActivity = {
  date?: string | null;
  kind: string;
  symbol?: string | null;
  units?: number | null;
  amount?: number | null;
  currency?: string | null;
  account: string;
  description?: string | null;
};

/**
 * `heiwa_finance_summary_v1`: the read-only finance read model. Heiwa reads
 * balances, positions, and transactions; nothing in it can trade.
 */
export type FinanceSummary = {
  schema_version: string;
  policy: "read_only";
  policy_note?: string;
  connections?: { brokerage: boolean; market_data: boolean };
  settings?: { base_currency: string; benchmark: string; tfsa_room?: { year: number; room_at_start: number } | null };
  sync?: { last_attempt_at?: string | null; last_success_at?: string | null; outcome?: string | null; issues?: FinanceIssue[] };
  fx?: Record<string, { rate: number; date: string; source: string }>;
  portfolio?: {
    base_currency: string;
    total_value: number;
    cash: number;
    invested: number;
    unrealized_gain?: number | null;
    accounts: FinanceAccount[];
    positions: FinancePosition[];
    unconverted: string[];
  } | null;
  tfsa?: {
    year: number;
    contributions_ytd: number;
    withdrawals_ytd: number;
    room_remaining?: number | null;
    room_status: string;
    trades_365d: number;
    quick_sells_365d: number;
    trading_level: string;
    us_listed_value: number;
    notes: string[];
  } | null;
  benchmark?: {
    benchmark: string;
    status: string;
    flows: number;
    flows_priced: number;
    net_contributed: number;
    shadow_value?: number | null;
    actual_value?: number | null;
    difference?: number | null;
    as_of?: string | null;
    notes: string[];
  } | null;
  activity?: { recent: FinanceActivity[]; total: number };
  freshness?: { snapshot_synced_at?: string | null; positions_as_of_oldest?: string | null; benchmark_bars_through?: string | null };
  next_actions?: string[];
  error?: string;
};

export type FinanceSyncResult = {
  outcome: "ok" | "partial" | "error";
  counts: Record<string, number>;
  issues: FinanceIssue[];
  receipt_id?: string | null;
};

/** Apple Reminders connection: enrollment and the saved list selection. */
export type ReminderStatus = {
  connector: "apple_reminders";
  status: "connected" | "disconnected" | "config_error";
  detail?: string;
  selected_list_ids?: string[];
};

export type ReminderList = { id: string; name: string; source: string; writable: boolean };

export type ReminderRow = {
  id: string;
  list_id: string;
  title: string;
  due: { date?: string; instant?: string } | null;
  completed: boolean;
};

/** One bounded read of the selected lists. `complete: false` means absence is not established. */
export type ReminderRead = { reminders: ReminderRow[]; selected_list_ids: string[]; truncated: boolean; complete: boolean };
