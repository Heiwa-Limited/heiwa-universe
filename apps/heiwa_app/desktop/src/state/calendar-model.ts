import { localIsoDate, parseLocalDate } from "../lib/format";
import type { CalendarEvent } from "./types";

/**
 * One calendar row in the only shape the views read.
 *
 * The runtime hands back three dialects: EventKit and Google rows carry UTC
 * instants, local holds carry a date plus `HH:MM` clock times, and register
 * appointments carry a bare date. Every view used to reinterpret those itself,
 * which is how a UTC instant ended up printed as a time and a date-only string
 * landed on the previous day. Normalizing once, into local `Date`s and local
 * day keys, leaves the views nothing to get wrong.
 */
export type AgendaEvent = {
  id: string;
  title: string;
  /** Calendar name, or a readable label for where the row came from. */
  calendar: string;
  source: string;
  kind: string;
  status: string;
  allDay: boolean;
  recurring: boolean;
  start: Date;
  /** Exclusive. An all-day event ends at local midnight after its last day. */
  end: Date;
  /** `YYYY-MM-DD` of the local day the event starts on. */
  startDay: string;
  /** `YYYY-MM-DD` of the last local day the event touches, inclusive. */
  endDay: string;
  note?: string;
};

const HOUR_MS = 3_600_000;
const HOLD_KINDS = new Set(["focus", "travel", "soft"]);
/** Longest span a single event is spread across in a day index. */
const MAX_INDEXED_DAYS = 62;

function atClock(day: Date, clock: string | undefined): Date | null {
  const match = clock?.match(/^(\d{1,2}):(\d{2})$/);
  if (!match) return null;
  const [hours, minutes] = [Number(match[1]), Number(match[2])];
  if (hours > 23 || minutes > 59) return null;
  return new Date(day.getFullYear(), day.getMonth(), day.getDate(), hours, minutes);
}

/** A full timestamp only; bare dates and clock times mean something else. */
function instant(raw: string | undefined): Date | null {
  if (!raw || !raw.includes("T")) return null;
  const parsed = new Date(raw);
  return Number.isNaN(parsed.getTime()) ? null : parsed;
}

function midnight(when: Date): Date {
  return new Date(when.getFullYear(), when.getMonth(), when.getDate());
}

export function addDays(day: Date, count: number): Date {
  return new Date(day.getFullYear(), day.getMonth(), day.getDate() + count);
}

export function addIsoDays(iso: string, count: number): string {
  const day = parseLocalDate(iso);
  return day ? localIsoDate(addDays(day, count)) : iso;
}

export function sourceLabel(source: string | undefined, kind?: string): string {
  if (source === "apple_calendar") return "Apple Calendar";
  if (source === "google_calendar") return "Google Calendar";
  if (source === "heiwa_hold" || (kind && HOLD_KINDS.has(kind))) return "Heiwa hold";
  if (source === "life_register") return "Life register";
  return "Calendar";
}

export function normalizeEvent(raw: CalendarEvent): AgendaEvent | null {
  const date = raw.date ? parseLocalDate(raw.date) : null;
  const startInstant = instant(raw.start);
  const startDate = raw.start ? parseLocalDate(raw.start) : null;
  let start: Date | null = null;
  let end: Date | null = null;
  let firstDay: Date | null = null;
  let lastDay: Date | null = null;

  const allDay = raw.all_day === true || Boolean(startDate) || (!raw.start && Boolean(date));
  if (allDay) {
    // The calendar's day identity wins over instants, which move with the zone.
    firstDay = date ?? startDate ?? (startInstant ? midnight(startInstant) : null);
    if (!firstDay) return null;
    const endInstant = instant(raw.end);
    const exclusiveEnd = raw.end ? parseLocalDate(raw.end) : null;
    lastDay = (raw.end_date ? parseLocalDate(raw.end_date) : null)
      ?? (exclusiveEnd ? addDays(exclusiveEnd, -1) : null)
      ?? (endInstant ? midnight(new Date(endInstant.getTime() - 1)) : null)
      ?? firstDay;
    if (lastDay < firstDay) lastDay = firstDay;
    start = firstDay;
    end = addDays(lastDay, 1);
  } else if (startInstant) {
    start = startInstant;
    end = instant(raw.end) ?? new Date(start.getTime() + HOUR_MS);
  } else if (date) {
    start = atClock(date, raw.start);
    if (!start) return null;
    end = atClock(date, raw.end) ?? new Date(start.getTime() + HOUR_MS);
  }
  if (!start || !end) return null;
  if (end < start) end = start;

  const source = raw.source ?? (raw.kind && HOLD_KINDS.has(raw.kind) ? "heiwa_hold" : "calendar");
  return {
    id: raw.id || `${source}:${raw.title ?? ""}:${raw.start ?? raw.date}`,
    title: raw.title?.trim() || "Untitled",
    calendar: raw.calendar || sourceLabel(source, raw.kind),
    source,
    kind: raw.kind || "event",
    status: raw.status || "confirmed",
    allDay,
    recurring: raw.recurring === true,
    start,
    end,
    startDay: localIsoDate(firstDay ?? start),
    endDay: localIsoDate(
      lastDay ?? (end > start ? new Date(end.getTime() - 1) : start),
    ),
    note: raw.note || undefined,
  };
}

export function compareEvents(left: AgendaEvent, right: AgendaEvent): number {
  return left.start.getTime() - right.start.getTime()
    || Number(right.allDay) - Number(left.allDay)
    || left.end.getTime() - right.end.getTime()
    || left.title.localeCompare(right.title);
}

function sameEvent(left: AgendaEvent, right: AgendaEvent): boolean {
  return left.title === right.title
    && left.calendar === right.calendar
    && left.source === right.source
    && left.kind === right.kind
    && left.status === right.status
    && left.allDay === right.allDay
    && left.recurring === right.recurring
    && left.start.getTime() === right.start.getTime()
    && left.end.getTime() === right.end.getTime()
    && left.note === right.note;
}

/**
 * Normalize, drop unreadable rows and duplicate ids, and order by time.
 *
 * An event unchanged since `previous` comes back as the same object. Views
 * key rows by identity, and the calendar reloads every minute while open; a
 * fresh object for every row would rebuild the list each time and throw away
 * keyboard focus and hover state.
 */
export function normalizeEvents(rows: CalendarEvent[], previous: readonly AgendaEvent[] = []): AgendaEvent[] {
  const prior = new Map(previous.map((event) => [event.id, event]));
  const seen = new Set<string>();
  const events: AgendaEvent[] = [];
  for (const row of rows) {
    const event = normalizeEvent(row);
    if (!event || seen.has(event.id)) continue;
    seen.add(event.id);
    const kept = prior.get(event.id);
    events.push(kept && sameEvent(kept, event) ? kept : event);
  }
  return events.sort(compareEvents);
}

export function occursOn(event: AgendaEvent, day: string): boolean {
  return event.startDay <= day && day <= event.endDay;
}

/** Events per local day within `[from, to]`, spreading multi-day events. */
export function indexByDay(events: AgendaEvent[], from: string, to: string): Map<string, AgendaEvent[]> {
  const index = new Map<string, AgendaEvent[]>();
  for (const event of events) {
    let day = event.startDay < from ? from : event.startDay;
    const last = event.endDay > to ? to : event.endDay;
    for (let step = 0; day <= last && step < MAX_INDEXED_DAYS; step += 1) {
      const bucket = index.get(day);
      if (bucket) bucket.push(event);
      else index.set(day, [event]);
      day = addIsoDays(day, 1);
    }
  }
  return index;
}

/** What has not ended yet, starting before the horizon, in time order. */
export function upcomingEvents(
  events: AgendaEvent[],
  now: Date,
  options: { days?: number; limit?: number } = {},
): AgendaEvent[] {
  const horizon = addDays(midnight(now), options.days ?? 14);
  return events
    .filter((event) => (event.end > now || (event.end.getTime() === event.start.getTime() && event.start >= now))
      && event.start < horizon)
    .sort(compareEvents)
    .slice(0, options.limit ?? 50);
}

/** Group time-ordered events under the first day still ahead of `fromDay`. */
export function groupByDay(events: AgendaEvent[], fromDay: string): Array<{ day: string; events: AgendaEvent[] }> {
  const groups = new Map<string, AgendaEvent[]>();
  for (const event of events) {
    const day = event.startDay < fromDay ? fromDay : event.startDay;
    const bucket = groups.get(day);
    if (bucket) bucket.push(event);
    else groups.set(day, [event]);
  }
  return Array.from(groups, ([day, grouped]) => ({ day, events: grouped }));
}

function clock(when: Date): string {
  return when.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}

function monthDay(when: Date): string {
  return when.toLocaleDateString([], { month: "short", day: "numeric" });
}

export function dayLabel(day: string, today: string): string {
  const date = parseLocalDate(day);
  const reference = parseLocalDate(today);
  if (!date || !reference) return day;
  const offset = Math.round((date.getTime() - reference.getTime()) / (24 * HOUR_MS));
  if (offset === 0) return "Today";
  if (offset === 1) return "Tomorrow";
  if (offset === -1) return "Yesterday";
  return date.toLocaleDateString([], {
    weekday: "short",
    month: "short",
    day: "numeric",
    year: date.getFullYear() === reference.getFullYear() ? undefined : "numeric",
  });
}

export function longDayLabel(day: string): string {
  const date = parseLocalDate(day);
  return date
    ? date.toLocaleDateString([], { weekday: "long", month: "long", day: "numeric", year: "numeric" })
    : day;
}

export function timeLabel(event: AgendaEvent): string {
  return event.allDay ? "All day" : clock(event.start);
}

/**
 * The time column for an event listed under `day`. An overnight shift carried
 * over from yesterday says when it ends; one covering the whole day is all day.
 */
export function agendaTimeLabel(event: AgendaEvent, day: string): string {
  if (event.allDay) return "All day";
  if (event.startDay < day) return event.endDay > day ? "All day" : `Until ${clock(event.end)}`;
  return clock(event.start);
}

export function timeRange(event: AgendaEvent): string {
  if (event.allDay) {
    return event.startDay === event.endDay
      ? "All day"
      : `${monthDay(event.start)} – ${monthDay(addDays(event.end, -1))}`;
  }
  if (event.end.getTime() === event.start.getTime()) return clock(event.start);
  return event.startDay === event.endDay
    ? `${clock(event.start)} – ${clock(event.end)}`
    : `${monthDay(event.start)}, ${clock(event.start)} – ${monthDay(event.end)}, ${clock(event.end)}`;
}

export function durationLabel(event: AgendaEvent): string {
  if (event.allDay) {
    const days = Math.round((event.end.getTime() - event.start.getTime()) / (24 * HOUR_MS));
    return days === 1 ? "1 day" : `${days} days`;
  }
  const minutes = Math.round((event.end.getTime() - event.start.getTime()) / 60_000);
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest ? `${hours} hr ${rest} min` : `${hours} hr`;
}

/** "Now", "in 45 min", "in 3 hr" today; empty when the day label says enough. */
export function relativeLabel(event: AgendaEvent, now: Date): string {
  if (event.allDay) return "";
  if (event.start <= now && now < event.end) return "Now";
  const minutes = Math.ceil((event.start.getTime() - now.getTime()) / 60_000);
  if (minutes <= 0) return "";
  if (minutes < 60) return `in ${minutes} min`;
  if (event.startDay === localIsoDate(now)) return `in ${Math.round(minutes / 60)} hr`;
  return "";
}

/** How long ago `then` was: "just now", "4 min ago", "2 hr ago", or a date. */
export function ageLabel(then: Date, now: Date): string {
  const seconds = Math.max(0, Math.round((now.getTime() - then.getTime()) / 1000));
  if (seconds < 60) return "just now";
  if (seconds < 3600) return `${Math.floor(seconds / 60)} min ago`;
  if (seconds < 86_400) return `${Math.floor(seconds / 3600)} hr ago`;
  return `on ${monthDay(then)}`;
}

/** Categorical colors the theme defines as `--series-1` … `--series-N`. */
export const SERIES_COUNT = 8;

/**
 * A stable theme series (1-based) per calendar, so the same calendar keeps its
 * color. The color itself stays in the theme layer.
 */
export function calendarSeries(name: string): number {
  let hash = 2166136261;
  for (let index = 0; index < name.length; index += 1) {
    hash ^= name.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return (Math.abs(hash) % SERIES_COUNT) + 1;
}

export type MonthCell = { day: number; iso: string; inMonth: boolean; isToday: boolean };

/** Whole weeks covering the cursor's month, including neighbouring days. */
export function monthGrid(cursor: Date, today: string): { weeks: MonthCell[][]; from: string; to: string } {
  const first = new Date(cursor.getFullYear(), cursor.getMonth(), 1);
  const gridStart = addDays(first, -first.getDay());
  const last = new Date(cursor.getFullYear(), cursor.getMonth() + 1, 0);
  const gridEnd = addDays(last, 6 - last.getDay());
  const weeks: MonthCell[][] = [];
  for (let day = gridStart; day <= gridEnd; day = addDays(day, 1)) {
    if (day.getDay() === 0) weeks.push([]);
    const iso = localIsoDate(day);
    weeks[weeks.length - 1].push({
      day: day.getDate(),
      iso,
      inMonth: day.getMonth() === cursor.getMonth(),
      isToday: iso === today,
    });
  }
  return { weeks, from: localIsoDate(gridStart), to: localIsoDate(gridEnd) };
}
