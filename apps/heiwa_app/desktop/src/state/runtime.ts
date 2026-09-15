import { createSignal, type Accessor } from "solid-js";
import {
  apiGet,
  apiPost,
  readAppleMail,
  runtimeHealth,
  type AppleMailScanResult,
  type RuntimeHealth,
} from "../runtime";
import { localIsoDate } from "../lib/format";
import type {
  ApprovalsSummary,
  CalendarEvent,
  CalendarRange,
  CalendarResources,
  CalendarSyncStatus,
  InboxItem,
  MailMessage,
} from "./types";

/** Runtime-derived state consumed by more than one surface. */
export type RuntimeState = {
  health: Accessor<RuntimeHealth | null>;
  /** Calendar rows overlapping `calendarRange`. */
  calendarEvents: Accessor<CalendarEvent[]>;
  calendarRange: Accessor<CalendarRange>;
  /** Set when the last calendar load failed; the previous rows stay visible. */
  calendarError: Accessor<string | undefined>;
  calendarSync: Accessor<CalendarSyncStatus | null>;
  calendarSyncing: Accessor<boolean>;
  /** An event another surface asked the Calendar to open. */
  calendarFocus: Accessor<string | undefined>;
  calendarResources: Accessor<CalendarResources | null>;
  approvals: Accessor<ApprovalsSummary | null>;
  inbox: Accessor<InboxItem[]>;
  /** Messages from the local Mail.app snapshot; empty until a scan runs. */
  mail: Accessor<MailMessage[]>;
  /** Whether the mail snapshot has been read at least once this session. */
  mailLoaded: Accessor<boolean>;
  mailError: Accessor<string | undefined>;
  loadHealth: () => Promise<void>;
  /** Load a local-day range, or reload the last one requested. */
  loadCalendar: (range?: CalendarRange) => Promise<void>;
  /**
   * Ask the runtime to re-read the selected Apple calendars if its copy is
   * older than `maxAgeSeconds` (or always, with `force`). Never throws: the
   * outcome, including failure, lands in `calendarSync`.
   */
  syncCalendar: (options?: { force?: boolean; maxAgeSeconds?: number }) => Promise<CalendarSyncStatus | null>;
  focusCalendarEvent: (id: string | undefined) => void;
  loadCalendarResources: () => Promise<void>;
  connectAppleCalendar: () => Promise<void>;
  readAppleCalendars: (ids: string[]) => Promise<{ fetched: number; truncated: boolean }>;
  disconnectAppleCalendar: () => Promise<void>;
  createCalendarHold: (input: CalendarHoldInput) => Promise<void>;
  loadApprovals: () => Promise<void>;
  decideApproval: (id: string, approve: boolean) => Promise<void>;
  loadInbox: () => Promise<void>;
  loadMail: () => Promise<void>;
  readAppleMail: () => Promise<AppleMailScanResult>;
};

export type CalendarHoldInput = {
  title: string;
  date: string;
  start: string;
  end: string;
  kind: "focus" | "travel" | "soft";
  promotion?: {
    connector: "apple_calendar";
    calendar: string;
  };
};

export type RuntimeStateOptions = {
  get?: typeof apiGet;
  post?: typeof apiPost;
  health?: typeof runtimeHealth;
  readAppleMail?: typeof readAppleMail;
};

type CalendarEventsResponse = { data?: { events?: CalendarEvent[]; sync?: CalendarSyncStatus } };
type CalendarSyncResponse = { data?: CalendarSyncStatus };
type InboxResponse = { data?: { items?: InboxItem[] } };

/** How old the runtime's calendar copy may be before a sync re-reads it. */
export const CALENDAR_SYNC_MAX_AGE_SECONDS = 120;

/** Yesterday through six weeks out: enough for Home and an upcoming list. */
export function defaultCalendarRange(now: Date = new Date()): CalendarRange {
  const day = (offset: number) =>
    localIsoDate(new Date(now.getFullYear(), now.getMonth(), now.getDate() + offset));
  return { from: day(-1), to: day(42) };
}
type MailResponse = { data?: { priority?: MailMessage[] } };
type CalendarResourcesResponse = { data?: CalendarResources };
type ApprovalsResponse = { data?: ApprovalsSummary };

export function createRuntimeState(options: RuntimeStateOptions = {}): RuntimeState {
  const get = options.get ?? apiGet;
  const post = options.post ?? apiPost;
  const health$ = options.health ?? runtimeHealth;
  const scanAppleMail = options.readAppleMail ?? readAppleMail;

  const [health, setHealth] = createSignal<RuntimeHealth | null>(null);
  const [calendarEvents, setCalendarEvents] = createSignal<CalendarEvent[]>([]);
  const [calendarRange, setCalendarRange] = createSignal<CalendarRange>(defaultCalendarRange());
  const [calendarError, setCalendarError] = createSignal<string>();
  const [calendarSync, setCalendarSync] = createSignal<CalendarSyncStatus | null>(null);
  const [calendarSyncing, setCalendarSyncing] = createSignal(false);
  const [calendarFocus, setCalendarFocus] = createSignal<string>();
  const [calendarResources, setCalendarResources] =
    createSignal<CalendarResources | null>(null);
  const [approvals, setApprovals] = createSignal<ApprovalsSummary | null>(null);
  const [inbox, setInbox] = createSignal<InboxItem[]>([]);
  const [mail, setMail] = createSignal<MailMessage[]>([]);
  const [mailLoaded, setMailLoaded] = createSignal(false);
  const [mailError, setMailError] = createSignal<string>();

  async function loadHealth(): Promise<void> {
    const next = await health$().catch(
      (): RuntimeHealth => ({ reachable: false, error: null, snapshot: null }),
    );
    setHealth(next);
  }

  // Month navigation can fire loads faster than the runtime answers; only the
  // newest request may write, or a slow September lands on top of October.
  let calendarRequest = 0;
  async function loadCalendar(range?: CalendarRange): Promise<void> {
    if (range) setCalendarRange(range);
    const { from, to } = range ?? calendarRange();
    const request = ++calendarRequest;
    try {
      const response = await get<CalendarEventsResponse>(
        `/api/v1/calendar/events?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`,
      );
      if (request !== calendarRequest) return;
      setCalendarEvents(response?.data?.events ?? []);
      if (response?.data?.sync) setCalendarSync(response.data.sync);
      setCalendarError(undefined);
    } catch {
      if (request !== calendarRequest) return;
      // A failed refresh must not blank a calendar the user is reading; the
      // next refresh retries.
      setCalendarError("The calendar could not be refreshed.");
    }
  }

  let syncInFlight: Promise<CalendarSyncStatus | null> | undefined;
  function syncCalendar(
    options: { force?: boolean; maxAgeSeconds?: number } = {},
  ): Promise<CalendarSyncStatus | null> {
    // Focus, the live interval, and "Sync now" can all ask at once. They share
    // one runtime read rather than queueing identical EventKit scans.
    if (syncInFlight) return syncInFlight;
    setCalendarSyncing(true);
    syncInFlight = (async () => {
      try {
        const response = await post<CalendarSyncResponse>("/api/v1/calendar/sync", {
          max_age_seconds: options.maxAgeSeconds ?? CALENDAR_SYNC_MAX_AGE_SECONDS,
          force: options.force ?? false,
        });
        const status = response?.data ?? null;
        setCalendarSync(status);
        if (status?.status === "synced") await loadCalendar();
        return status;
      } catch {
        const status: CalendarSyncStatus = {
          ...(calendarSync() ?? {}),
          status: "error",
          error: "Calendar sync could not reach the Heiwa runtime.",
        };
        setCalendarSync(status);
        return status;
      } finally {
        syncInFlight = undefined;
        setCalendarSyncing(false);
      }
    })();
    return syncInFlight;
  }

  async function loadCalendarResources(): Promise<void> {
    try {
      const response = await get<CalendarResourcesResponse>("/api/v1/calendar/resources");
      setCalendarResources(response?.data ?? null);
    } catch {
      // Keep the last known connection. Dropping to null on a transient
      // failure flips a connected calendar back to "Checking…" and hides it.
    }
  }

  async function connectAppleCalendar(): Promise<void> {
    await post("/api/v1/connectors/apple_calendar/connect", {});
    await loadCalendarResources();
  }

  async function readAppleCalendars(ids: string[]): Promise<{ fetched: number; truncated: boolean }> {
    const response = await post<{ data: { fetched: number; truncated: boolean } }>("/api/v1/calendar/read", { calendar_ids: ids });
    await Promise.all([loadCalendar(), loadCalendarResources()]);
    return response.data;
  }

  async function disconnectAppleCalendar(): Promise<void> {
    await post("/api/v1/connectors/apple_calendar/disconnect", {});
    await loadCalendarResources();
  }

  async function createCalendarHold(input: CalendarHoldInput): Promise<void> {
    await post("/api/v1/calendar/holds", input);
    await Promise.all([loadCalendar(), loadApprovals()]);
  }

  async function loadApprovals(): Promise<void> {
    try {
      const response = await get<ApprovalsResponse>("/api/v1/approvals/summary");
      setApprovals(response?.data ?? null);
    } catch {
      // Keep the last known queue: a failed refresh reading as "0 pending"
      // would tell the user there is nothing to decide.
    }
  }

  async function decideApproval(id: string, approve: boolean): Promise<void> {
    await post(`/api/v1/approvals/${encodeURIComponent(id)}/${approve ? "approve" : "deny"}`, {});
    await loadApprovals();
    await loadCalendar();
  }

  async function loadMailSnapshot(propagateError: boolean): Promise<void> {
    try {
      const response = await get<MailResponse>("/api/v1/mail/summary");
      setMail(response?.data?.priority ?? []);
      setMailError(undefined);
    } catch {
      // A failed refresh must not erase a snapshot the user was already
      // reading. The next explicit read can be retried from the surface.
      const detail = "The local Mail snapshot could not be loaded.";
      setMailError(detail);
      if (propagateError) throw new Error(detail);
    } finally {
      // Marked loaded either way: "the snapshot is empty" and "the request
      // failed" both mean there is nothing to show, and the surface has to
      // stop saying "loading" in both cases.
      setMailLoaded(true);
    }
  }

  async function loadMail(): Promise<void> {
    await loadMailSnapshot(false);
  }

  async function readAppleMailSnapshot(): Promise<AppleMailScanResult> {
    const result = await scanAppleMail();
    await loadMailSnapshot(true);
    return result;
  }

  async function loadInbox(): Promise<void> {
    try {
      const response = await get<InboxResponse>("/api/v1/inbox");
      setInbox(response?.data?.items ?? []);
    } catch {
      // Keep the last known rows; the legacy event socket retries on change.
    }
  }

  return {
    health,
    calendarEvents,
    calendarRange,
    calendarError,
    calendarSync,
    calendarSyncing,
    calendarFocus,
    syncCalendar,
    focusCalendarEvent: setCalendarFocus,
    calendarResources,
    approvals,
    inbox,
    mail,
    mailLoaded,
    mailError,
    loadHealth,
    loadCalendar,
    loadCalendarResources,
    connectAppleCalendar,
    readAppleCalendars,
    disconnectAppleCalendar,
    createCalendarHold,
    loadApprovals,
    decideApproval,
    loadInbox,
    loadMail,
    readAppleMail: readAppleMailSnapshot,
  };
}
