/** Shared shapes for data the runtime serves to more than one surface. */

export type CalendarEvent = {
  id?: string;
  title?: string;
  start?: string;
  end?: string;
  date?: string;
  kind?: string;
  status?: string;
  source?: string;
  note?: string;
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
  agent: string;
  server: string;
  state: string;
  pinnedPane: string;
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
