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
  reader_available?: boolean;
  detail?: string | null;
  next_action?: string | null;
};

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
  surface: "calendar" | "mail" | null;
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
