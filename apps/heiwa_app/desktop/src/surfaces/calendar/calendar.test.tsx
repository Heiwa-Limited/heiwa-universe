// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { afterAll, afterEach, beforeAll, expect, it, vi } from "vitest";
import { createRoot } from "solid-js";
import { localIsoDate } from "../../lib/format";
import { AppProvider, createAppState, type AppState } from "../../state/app";
import type { CalendarEvent } from "../../state/types";
import { calendarSurface } from "./index";

// West of UTC, where a UTC instant or a date-only string most easily lands on
// the wrong day. CI runs in UTC, so the zone is pinned.
beforeAll(() => {
  vi.stubEnv("TZ", "America/Vancouver");
});
afterAll(() => {
  vi.unstubAllEnvs();
});

let dispose: (() => void) | undefined;
afterEach(() => {
  cleanup();
  dispose?.();
  vi.useRealTimers();
});

const clock = (date: Date) => date.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
const local = (day: number, hour: number, minute = 0, month = 8) => new Date(2026, month, day, hour, minute);

type RuntimeOverrides = {
  events?: CalendarEvent[];
  resources?: unknown;
  get?: (path: string) => unknown;
  post?: (path: string, body: unknown) => Promise<unknown>;
};

function mount(overrides: RuntimeOverrides = {}): { state: AppState; get: ReturnType<typeof vi.fn> } {
  const get = vi.fn(async (path: string) => overrides.get?.(path) ?? ({
    data: path.startsWith("/api/v1/calendar/events?")
      ? { events: overrides.events ?? [] }
      : path.endsWith("/resources")
        ? overrides.resources ?? {}
        : {},
  }));
  const state = createRoot((cleanupRoot) => {
    dispose = cleanupRoot;
    return createAppState({
      runtime: {
        get: get as never,
        post: (overrides.post ?? (async () => ({ data: {} }))) as never,
      },
    });
  });
  // The shell's arrival refresh loads the connection; mounting directly skips it.
  void state.runtime.loadCalendarResources();
  const Calendar = calendarSurface.Component;
  render(() => <AppProvider state={state}><Calendar /></AppProvider>);
  return { state, get };
}

const week = (): CalendarEvent[] => [
  { id: "past", title: "Kickoff", source: "apple_calendar", calendar: "Work", date: "2026-09-01", start: "2026-09-01T16:00:00Z", end: "2026-09-01T17:00:00Z" },
  { id: "done", title: "Morning run", source: "apple_calendar", calendar: "Health", start: local(11, 7).toISOString(), end: local(11, 8).toISOString() },
  // 01:00 UTC on the 12th is 18:00 on the 11th in Vancouver.
  { id: "dinner", title: "Dinner", source: "apple_calendar", calendar: "Life", date: "2026-09-11", start: "2026-09-12T01:00:00Z", end: "2026-09-12T02:30:00Z" },
  { id: "day-off", title: "Day off", source: "apple_calendar", calendar: "Life", all_day: true, date: "2026-09-12", end_date: "2026-09-12" },
  { id: "october", title: "October review", source: "apple_calendar", calendar: "Work", start: local(2, 10, 0, 9).toISOString(), end: local(2, 11, 0, 9).toISOString() },
];

function agenda(): HTMLElement {
  return screen.getByRole("region", { name: /agenda/i });
}

function titles(): string[] {
  return Array.from(agenda().querySelectorAll(".cal-agenda-title"), (node) => node.textContent ?? "");
}

it("lists what is actually coming up, under the right local day and time", async () => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(local(11, 12));
  const { state } = mount({ events: week() });
  await state.runtime.loadCalendar();

  await waitFor(() => expect(titles()).toEqual(["Dinner", "Day off"]));
  expect(within(agenda()).getByRole("heading", { name: "Upcoming" })).toBeTruthy();
  const days = Array.from(agenda().querySelectorAll(".cal-agenda-day"), (node) => node.textContent);
  expect(days).toEqual(["Today", "Tomorrow"]);
  const dinner = screen.getByRole("button", { name: /Dinner/ });
  expect(dinner.textContent).toContain(clock(local(11, 18)));
  expect(screen.getByRole("button", { name: /Day off/ }).textContent).toContain("All day");
});

it("opens an event's details from the list and closes them again", async () => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(local(11, 12));
  const { state } = mount({ events: week() });
  await state.runtime.loadCalendar();

  const row = await screen.findByRole("button", { name: /Dinner/ });
  expect(row.getAttribute("aria-expanded")).toBe("false");
  fireEvent.click(row);

  expect(row.getAttribute("aria-expanded")).toBe("true");
  const details = screen.getByRole("region", { name: "Dinner details" });
  expect(details.textContent).toContain(`${clock(local(11, 18))} – ${clock(local(11, 19, 30))}`);
  expect(details.textContent).toContain("1 hr 30 min");
  expect(details.textContent).toContain(local(11, 0).toLocaleDateString([], { weekday: "long", month: "long", day: "numeric", year: "numeric" }));
  expect(details.textContent).toContain("Life");
  expect(details.textContent).toContain("Apple Calendar");

  fireEvent.keyDown(details, { key: "Escape" });
  expect(row.getAttribute("aria-expanded")).toBe("false");
  expect(screen.queryByRole("region", { name: "Dinner details" })).toBeNull();
});

it("shows a picked day's events, including ones already past", async () => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(local(11, 12));
  const { state } = mount({ events: week() });
  await state.runtime.loadCalendar();

  const firstOfMonth = local(1, 0).toLocaleDateString([], { weekday: "long", month: "long", day: "numeric", year: "numeric" });
  fireEvent.click(await screen.findByRole("button", { name: `${firstOfMonth}, 1 event` }));

  expect(within(agenda()).getByRole("heading", { name: firstOfMonth })).toBeTruthy();
  expect(titles()).toEqual(["Kickoff"]);

  fireEvent.click(within(agenda()).getByRole("button", { name: "Back to upcoming" }));
  expect(titles()).toEqual(["Dinner", "Day off"]);
});

it("loads the month being viewed and lists that month when today is not in it", async () => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(local(11, 12));
  const { get } = mount({ events: week() });

  // September's grid runs Aug 30 – Oct 3, which already covers two weeks ahead.
  await waitFor(() => expect(get).toHaveBeenCalledWith("/api/v1/calendar/events?from=2026-08-30&to=2026-10-03"));
  fireEvent.click(screen.getByRole("button", { name: "Next month" }));

  await waitFor(() => expect(get).toHaveBeenLastCalledWith("/api/v1/calendar/events?from=2026-09-27&to=2026-10-31"));
  expect(within(agenda()).getByRole("heading", { name: "October 2026" })).toBeTruthy();
  await waitFor(() => expect(titles()).toContain("October review"));
});

it("syncs on request and says how fresh the calendar is", async () => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(local(11, 12));
  const post = vi.fn(async () => ({
    data: { status: "synced", complete: true, fetched: 12, last_read_at: local(11, 12).toISOString() },
  }));
  mount({
    events: week(),
    post,
    resources: { source: "apple_calendar", status: "ready", calendars: [], selected_ids: ["work"] },
  });

  fireEvent.click(await screen.findByRole("button", { name: "Sync now" }));

  expect(post).toHaveBeenCalledWith("/api/v1/calendar/sync", { max_age_seconds: 120, force: true });
  const status = screen.getByRole("status", { name: "Calendar sync" });
  expect(status.textContent).toBe("Syncing…");
  await waitFor(() => expect(status.textContent).toBe("Updated just now"));
});

it("imports only explicitly selected calendar identities and shows the loaded events", async () => {
  const today = localIsoDate();
  let imported = false;
  const post = vi.fn(async (_path: string, _body: unknown) => {
    imported = true;
    return { data: { fetched: 1, truncated: false } };
  });
  mount({
    post,
    get: (path) => ({
      data: path.endsWith("/resources")
        ? {
            source: "apple_calendar", status: "ready", reader_available: true, selected_ids: imported ? ["work-id"] : [],
            calendars: [{ id: "work-id", name: "Work", source: "iCloud", writable: true }, { id: "private-id", name: "Private", writable: true }],
          }
        : path.startsWith("/api/v1/calendar/events?")
          ? { events: imported ? [{ id: "actual-event", date: today, all_day: true, title: "Imported meeting" }] : [] }
          : {},
    }),
  });
  const read = await screen.findByRole("button", { name: "Sync selected calendars" });
  await waitFor(() => expect(screen.getByRole("checkbox", { name: "Work · iCloud" })).toBeTruthy());
  expect((read as HTMLButtonElement).disabled).toBe(true);
  expect(post).not.toHaveBeenCalled();

  fireEvent.click(screen.getByRole("checkbox", { name: "Work · iCloud" }));
  fireEvent.click(read);

  await waitFor(() => expect(screen.getByText("Imported meeting")).toBeTruthy());
  expect(post).toHaveBeenCalledExactlyOnceWith("/api/v1/calendar/read", { calendar_ids: ["work-id"] });
  expect((screen.getByRole("checkbox", { name: "Private" }) as HTMLInputElement).checked).toBe(false);
});
