import { createMemo, createSignal, For, Show } from "solid-js";
import { shortDate } from "../../lib/format";
import { useApp } from "../../state/app";
import type { SurfaceModule } from "../types";
import "./calendar.css";

const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

function pad(value: number): string {
  return String(value).padStart(2, "0");
}

function CalendarSurface() {
  const app = useApp();
  // Month cursor is surface-local: no other surface needs it.
  const [cursor, setCursor] = createSignal(new Date());

  const grid = createMemo(() => {
    const date = cursor();
    const year = date.getFullYear();
    const month = date.getMonth();
    const leading = new Date(year, month, 1).getDay();
    const days = new Date(year, month + 1, 0).getDate();
    const today = new Date();
    const isCurrentMonth = today.getFullYear() === year && today.getMonth() === month;

    const cells: Array<{ day: number | null; iso: string; isToday: boolean }> = [];
    for (let i = 0; i < leading; i += 1) cells.push({ day: null, iso: "", isToday: false });
    for (let day = 1; day <= days; day += 1) {
      cells.push({
        day,
        iso: `${year}-${pad(month + 1)}-${pad(day)}`,
        isToday: isCurrentMonth && today.getDate() === day,
      });
    }
    return {
      cells,
      monthName: date.toLocaleDateString("en-US", { month: "long", year: "numeric" }),
      monthStart: `${year}-${pad(month + 1)}-01`,
    };
  });

  const upcoming = createMemo(() =>
    app.runtime
      .calendarEvents()
      .filter((event) => event.date && event.date >= grid().monthStart)
      .slice(0, 10),
  );

  const eventsOn = (iso: string) =>
    app.runtime.calendarEvents().filter((event) => event.date === iso);

  return (
    <div class="view calendar-view">
      <div class="cal-header">
        <div class="cal-nav">
          <button
            class="small-action"
            aria-label="Previous month"
            onClick={() => setCursor((d) => new Date(d.getFullYear(), d.getMonth() - 1, 1))}
          >
            ‹
          </button>
          <h2>{grid().monthName}</h2>
          <button
            class="small-action"
            aria-label="Next month"
            onClick={() => setCursor((d) => new Date(d.getFullYear(), d.getMonth() + 1, 1))}
          >
            ›
          </button>
          <button class="small-action" onClick={() => setCursor(new Date())}>
            Today
          </button>
        </div>
      </div>

      <div class="cal-grid" role="grid">
        <For each={WEEKDAYS}>{(day) => <div class="cal-weekday">{day}</div>}</For>
        <For each={grid().cells}>
          {(cell) => (
            <div class="cal-cell" classList={{ empty: cell.day === null, today: cell.isToday }}>
              <Show when={cell.day !== null}>
                <span class="cal-day">{cell.day}</span>
                <div class="cal-dots">
                  <For each={eventsOn(cell.iso).slice(0, 3)}>
                    {(event) => <span class={`cal-dot ${event.kind || "event"}`} />}
                  </For>
                </div>
              </Show>
            </div>
          )}
        </For>
      </div>

      <div class="panel cal-upcoming">
        <header>
          <span>Upcoming</span>
          <strong>{upcoming().length}</strong>
        </header>
        <Show
          when={upcoming().length > 0}
          fallback={<div class="empty-state">No events this month.</div>}
        >
          <For each={upcoming()}>
            {(event) => (
              <div class="cal-event-row">
                <span class="cal-event-date quiet">{shortDate(event.date)}</span>
                <span class="cal-event-title">{event.title || "Untitled"}</span>
                <span class="cal-event-kind quiet">{event.kind || ""}</span>
              </div>
            )}
          </For>
        </Show>
      </div>
    </div>
  );
}

export const calendarSurface: SurfaceModule = {
  id: "calendar",
  label: "Calendar",
  glyph: "◷",
  caption: "calendar window",
  Component: CalendarSurface,
  preview: (app) => {
    const next = app.runtime
      .calendarEvents()
      .filter((event) => event.date || event.start)
      .sort((a, b) => String(a.start || a.date).localeCompare(String(b.start || b.date)))[0];
    return {
      title: "Calendar",
      lines: [
        next
          ? `${shortDate(next.start || next.date)} ${next.title || "Untitled"}`
          : "no loaded events",
        `${app.runtime.calendarEvents().length} local items`,
      ],
    };
  },
  refresh: (app) => app.runtime.loadCalendar(),
};
