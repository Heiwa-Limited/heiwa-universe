import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import {
  agendaTimeLabel,
  dayLabel,
  groupByDay,
  normalizeEvent,
  normalizeEvents,
  occursOn,
  relativeLabel,
  timeLabel,
  timeRange,
  upcomingEvents,
} from "./calendar-model";

// Pinned west of UTC: every day-boundary mistake this model exists to prevent
// is invisible when the test machine runs in UTC.
beforeAll(() => {
  vi.stubEnv("TZ", "America/Vancouver");
});
afterAll(() => {
  vi.unstubAllEnvs();
});

const at = (hour: number, minute = 0, day = 14) => new Date(2026, 8, day, hour, minute);
const clock = (hour: number, minute = 0) =>
  at(hour, minute).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });

describe("normalizeEvent", () => {
  it("files an EventKit instant under the local day it happens on", () => {
    // 01:00 UTC on the 15th is 18:00 on the 14th in Vancouver.
    const event = normalizeEvent({
      id: "evt_1",
      title: "Dinner",
      calendar: "Life",
      source: "apple_calendar",
      date: "2026-09-14",
      start: "2026-09-15T01:00:00Z",
      end: "2026-09-15T01:45:00Z",
      all_day: false,
    });
    expect(event?.startDay).toBe("2026-09-14");
    expect(event?.endDay).toBe("2026-09-14");
    expect(timeLabel(event!)).toBe(clock(18));
    expect(timeRange(event!)).toBe(`${clock(18)} – ${clock(18, 45)}`);
  });

  it("reads a local hold's clock times on its own date", () => {
    const hold = normalizeEvent({ id: "hold-1", title: "Focus", date: "2026-09-14", start: "13:00", end: "14:30", kind: "focus" });
    expect(hold?.start.getTime()).toBe(at(13).getTime());
    expect(hold?.end.getTime()).toBe(at(14, 30).getTime());
    expect(hold?.allDay).toBe(false);
  });

  it("keeps all-day and date-only events on their calendar days", () => {
    const trip = normalizeEvent({ id: "trip", title: "Trip", all_day: true, date: "2026-09-12", end_date: "2026-09-16", start: "2026-09-12T07:00:00Z", end: "2026-09-17T06:59:59Z" });
    expect([trip!.startDay, trip!.endDay]).toEqual(["2026-09-12", "2026-09-16"]);
    expect(occursOn(trip!, "2026-09-16")).toBe(true);
    expect(occursOn(trip!, "2026-09-17")).toBe(false);
    expect(timeLabel(trip!)).toBe("All day");

    // Google's all-day end date is exclusive.
    const google = normalizeEvent({ id: "g", title: "Holiday", source: "google_calendar", start: "2026-09-20", end: "2026-09-21" });
    expect([google!.allDay, google!.startDay, google!.endDay]).toEqual([true, "2026-09-20", "2026-09-20"]);

    const appointment = normalizeEvent({ title: "dental appointment", date: "2026-09-14" });
    expect([appointment!.allDay, appointment!.startDay]).toEqual([true, "2026-09-14"]);
  });

  it("drops rows that carry no usable time", () => {
    expect(normalizeEvent({ id: "x", title: "Nowhere" })).toBeNull();
    expect(normalizeEvent({ id: "y", title: "Bad", start: "soon" })).toBeNull();
  });

  it("hands back the same object for an unchanged event, so a live refresh keeps its row", () => {
    const rows = [
      { id: "a", title: "Standup", start: at(9).toISOString(), end: at(9, 15).toISOString() },
      { id: "b", title: "Review", start: at(10).toISOString(), end: at(11).toISOString() },
    ];
    const first = normalizeEvents(rows);
    const again = normalizeEvents(rows.map((row) => ({ ...row })), first);
    expect(again[0]).toBe(first[0]);

    const renamed = normalizeEvents([rows[0], { ...rows[1], title: "Design review" }], first);
    expect(renamed[1]).not.toBe(first[1]);
    expect(renamed[1].title).toBe("Design review");
  });

  it("keeps a timed event that ends at midnight on its starting day", () => {
    const late = normalizeEvent({ id: "late", title: "Late", start: at(23).toISOString(), end: at(0, 0, 15).toISOString() });
    expect(late?.endDay).toBe("2026-09-14");
  });
});

describe("upcoming", () => {
  const events = normalizeEvents([
    { id: "done", title: "Done", start: at(9).toISOString(), end: at(11).toISOString() },
    { id: "hold", title: "Hold", date: "2026-09-14", start: "13:00", end: "14:00", kind: "focus" },
    { id: "ongoing", title: "Ongoing", start: at(11, 30).toISOString(), end: at(12, 30).toISOString() },
    { id: "soon", title: "Soon", start: at(12, 45).toISOString(), end: at(13, 15).toISOString() },
    { id: "trip", title: "Trip", all_day: true, date: "2026-09-12", end_date: "2026-09-16" },
    { id: "tomorrow", title: "Tomorrow", start: at(9, 0, 15).toISOString(), end: at(10, 0, 15).toISOString() },
    { id: "far", title: "Far", start: at(9, 0, 30).toISOString(), end: at(10, 0, 30).toISOString() },
  ]);
  const now = at(12);

  it("lists what has not ended yet, in the order it happens", () => {
    expect(upcomingEvents(events, now, { days: 7 }).map((event) => event.id)).toEqual([
      "trip",
      "ongoing",
      "soon",
      "hold",
      "tomorrow",
    ]);
  });

  it("groups under the first day still ahead, so an ongoing trip sits under today", () => {
    const groups = groupByDay(upcomingEvents(events, now, { days: 7 }), "2026-09-14");
    expect(groups.map((group) => [group.day, group.events.map((event) => event.id)])).toEqual([
      ["2026-09-14", ["trip", "ongoing", "soon", "hold"]],
      ["2026-09-15", ["tomorrow"]],
    ]);
  });

  it("describes timing relative to now", () => {
    const byId = new Map(events.map((event) => [event.id, event]));
    expect(relativeLabel(byId.get("ongoing")!, now)).toBe("Now");
    expect(relativeLabel(byId.get("soon")!, now)).toBe("in 45 min");
    expect(relativeLabel(byId.get("tomorrow")!, now)).toBe("");
  });
});

describe("agendaTimeLabel", () => {
  it("says when a carried-over event ends instead of repeating yesterday's start", () => {
    const shift = normalizeEvent({ id: "shift", title: "Shift", start: at(23, 30, 13).toISOString(), end: at(0, 15).toISOString() })!;
    expect(agendaTimeLabel(shift, "2026-09-13")).toBe(clock(23, 30));
    expect(agendaTimeLabel(shift, "2026-09-14")).toBe(`Until ${clock(0, 15)}`);

    const conference = normalizeEvent({ id: "conf", title: "Conference", start: at(9, 0, 13).toISOString(), end: at(17, 0, 15).toISOString() })!;
    expect(agendaTimeLabel(conference, "2026-09-14")).toBe("All day");
  });
});

describe("dayLabel", () => {
  it("names nearby days and dates the rest", () => {
    expect(dayLabel("2026-09-14", "2026-09-14")).toBe("Today");
    expect(dayLabel("2026-09-15", "2026-09-14")).toBe("Tomorrow");
    expect(dayLabel("2026-09-13", "2026-09-14")).toBe("Yesterday");
    expect(dayLabel("2026-09-16", "2026-09-14")).toBe(
      new Date(2026, 8, 16).toLocaleDateString([], { weekday: "short", month: "short", day: "numeric" }),
    );
  });
});
