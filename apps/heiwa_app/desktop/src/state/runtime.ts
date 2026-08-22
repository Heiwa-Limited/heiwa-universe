import { createSignal, type Accessor } from "solid-js";
import { apiGet, apiPost, runtimeHealth, type RuntimeHealth } from "../runtime";
import type {
  ApprovalsSummary,
  CalendarEvent,
  CalendarResources,
  InboxItem,
  MailMessage,
} from "./types";

/** Runtime-derived state consumed by more than one surface. */
export type RuntimeState = {
  health: Accessor<RuntimeHealth | null>;
  calendarEvents: Accessor<CalendarEvent[]>;
  calendarResources: Accessor<CalendarResources | null>;
  approvals: Accessor<ApprovalsSummary | null>;
  inbox: Accessor<InboxItem[]>;
  /** Messages from the local Mail.app snapshot; empty until a scan runs. */
  mail: Accessor<MailMessage[]>;
  /** Whether the mail snapshot has been read at least once this session. */
  mailLoaded: Accessor<boolean>;
  loadHealth: () => Promise<void>;
  loadCalendar: () => Promise<void>;
  loadCalendarResources: () => Promise<void>;
  connectAppleCalendar: () => Promise<void>;
  disconnectAppleCalendar: () => Promise<void>;
  createCalendarHold: (input: CalendarHoldInput) => Promise<void>;
  loadApprovals: () => Promise<void>;
  decideApproval: (id: string, approve: boolean) => Promise<void>;
  loadInbox: () => Promise<void>;
  loadMail: () => Promise<void>;
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
};

type CalendarSummary = { data?: { holds?: CalendarEvent[]; events?: CalendarEvent[] } };
type LifeToday = {
  data?: { calendar?: { holds?: CalendarEvent[] }; appointments?: CalendarEvent[] };
};
type InboxResponse = { data?: { items?: InboxItem[] } };
type MailResponse = { data?: { priority?: MailMessage[] } };
type CalendarResourcesResponse = { data?: CalendarResources };
type ApprovalsResponse = { data?: ApprovalsSummary };

export function createRuntimeState(options: RuntimeStateOptions = {}): RuntimeState {
  const get = options.get ?? apiGet;
  const post = options.post ?? apiPost;
  const health$ = options.health ?? runtimeHealth;

  const [health, setHealth] = createSignal<RuntimeHealth | null>(null);
  const [calendarEvents, setCalendarEvents] = createSignal<CalendarEvent[]>([]);
  const [calendarResources, setCalendarResources] =
    createSignal<CalendarResources | null>(null);
  const [approvals, setApprovals] = createSignal<ApprovalsSummary | null>(null);
  const [inbox, setInbox] = createSignal<InboxItem[]>([]);
  const [mail, setMail] = createSignal<MailMessage[]>([]);
  const [mailLoaded, setMailLoaded] = createSignal(false);

  async function loadHealth(): Promise<void> {
    const next = await health$().catch(
      (): RuntimeHealth => ({ reachable: false, error: null, snapshot: null }),
    );
    setHealth(next);
  }

  async function loadCalendar(): Promise<void> {
    try {
      const [summary, today] = await Promise.all([
        get<CalendarSummary>("/api/v1/calendar/summary"),
        get<LifeToday>("/api/v1/life/today"),
      ]);
      const merged = [
        ...(summary?.data?.holds ?? []),
        ...(summary?.data?.events ?? []),
        ...(today?.data?.calendar?.holds ?? []),
        ...(today?.data?.appointments ?? []),
      ];
      const seen = new Set<string>();
      setCalendarEvents(
        merged.filter((event) => {
          const key = event.id || `${event.title}-${event.start}-${event.date}`;
          if (seen.has(key)) return false;
          seen.add(key);
          return true;
        }),
      );
    } catch {
      setCalendarEvents([]);
    }
  }

  async function loadCalendarResources(): Promise<void> {
    try {
      const response = await get<CalendarResourcesResponse>("/api/v1/calendar/resources");
      setCalendarResources(response?.data ?? null);
    } catch {
      setCalendarResources(null);
    }
  }

  async function connectAppleCalendar(): Promise<void> {
    await post("/api/v1/connectors/apple_calendar/connect", {});
    await loadCalendarResources();
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
      setApprovals(null);
    }
  }

  async function decideApproval(id: string, approve: boolean): Promise<void> {
    await post(`/api/v1/approvals/${encodeURIComponent(id)}/${approve ? "approve" : "deny"}`, {});
    await loadApprovals();
    await loadCalendar();
  }

  async function loadMail(): Promise<void> {
    try {
      const response = await get<MailResponse>("/api/v1/mail/summary");
      setMail(response?.data?.priority ?? []);
    } catch {
      setMail([]);
    } finally {
      // Marked loaded either way: "the snapshot is empty" and "the request
      // failed" both mean there is nothing to show, and the surface has to
      // stop saying "loading" in both cases.
      setMailLoaded(true);
    }
  }

  async function loadInbox(): Promise<void> {
    try {
      const response = await get<InboxResponse>("/api/v1/inbox");
      setInbox(response?.data?.items ?? []);
    } catch {
      setInbox([]);
    }
  }

  return {
    health,
    calendarEvents,
    calendarResources,
    approvals,
    inbox,
    mail,
    mailLoaded,
    loadHealth,
    loadCalendar,
    loadCalendarResources,
    connectAppleCalendar,
    disconnectAppleCalendar,
    createCalendarHold,
    loadApprovals,
    decideApproval,
    loadInbox,
    loadMail,
  };
}
