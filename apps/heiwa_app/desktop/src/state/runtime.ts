import { createSignal, type Accessor } from "solid-js";
import { apiGet, runtimeHealth, type RuntimeHealth } from "../runtime";
import type { CalendarEvent, InboxItem } from "./types";

/** Runtime-derived state consumed by more than one surface. */
export type RuntimeState = {
  health: Accessor<RuntimeHealth | null>;
  calendarEvents: Accessor<CalendarEvent[]>;
  inbox: Accessor<InboxItem[]>;
  loadHealth: () => Promise<void>;
  loadCalendar: () => Promise<void>;
  loadInbox: () => Promise<void>;
};

export type RuntimeStateOptions = {
  get?: typeof apiGet;
  health?: typeof runtimeHealth;
};

type CalendarSummary = { data?: { holds?: CalendarEvent[]; events?: CalendarEvent[] } };
type LifeToday = {
  data?: { calendar?: { holds?: CalendarEvent[] }; appointments?: CalendarEvent[] };
};
type InboxResponse = { data?: { items?: InboxItem[] } };

export function createRuntimeState(options: RuntimeStateOptions = {}): RuntimeState {
  const get = options.get ?? apiGet;
  const health$ = options.health ?? runtimeHealth;

  const [health, setHealth] = createSignal<RuntimeHealth | null>(null);
  const [calendarEvents, setCalendarEvents] = createSignal<CalendarEvent[]>([]);
  const [inbox, setInbox] = createSignal<InboxItem[]>([]);

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

  async function loadInbox(): Promise<void> {
    try {
      const response = await get<InboxResponse>("/api/v1/inbox");
      setInbox(response?.data?.items ?? []);
    } catch {
      setInbox([]);
    }
  }

  return { health, calendarEvents, inbox, loadHealth, loadCalendar, loadInbox };
}
