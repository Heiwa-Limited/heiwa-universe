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
  const [connectionBusy, setConnectionBusy] = createSignal(false);
  const [connectionError, setConnectionError] = createSignal<string | null>(null);
  const [title, setTitle] = createSignal("");
  const [date, setDate] = createSignal("");
  const [start, setStart] = createSignal("");
  const [end, setEnd] = createSignal("");
  const [stagingBusy, setStagingBusy] = createSignal(false);
  const [stagingError, setStagingError] = createSignal<string | null>(null);
  const [stagingNotice, setStagingNotice] = createSignal<string | null>(null);

  const writableCalendars = createMemo(
    () => app.runtime.calendarResources()?.calendars.filter((calendar) => calendar.writable) ?? [],
  );
  const [selectedCalendar, setSelectedCalendar] = createSignal("");

  function promotionCalendar(): string {
    const selected = selectedCalendar();
    return writableCalendars().some((calendar) => calendar.name === selected)
      ? selected
      : writableCalendars()[0]?.name ?? "";
  }

  async function setAppleConnection(connect: boolean): Promise<void> {
    if (connectionBusy()) return;
    setConnectionBusy(true);
    setConnectionError(null);
    try {
      if (connect) await app.runtime.connectAppleCalendar();
      else await app.runtime.disconnectAppleCalendar();
    } catch (cause) {
      setConnectionError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setConnectionBusy(false);
    }
  }

  async function stageAppleEvent(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (stagingBusy()) return;
    const calendar = promotionCalendar();
    if (!title().trim() || !date() || !start() || !end() || !calendar) {
      setStagingError("Add a title, date, start, end, and writable calendar.");
      return;
    }
    setStagingBusy(true);
    setStagingError(null);
    setStagingNotice(null);
    try {
      await app.runtime.createCalendarHold({
        title: title().trim(),
        date: date(),
        start: start(),
        end: end(),
        kind: "focus",
        promotion: { connector: "apple_calendar", calendar },
      });
      setTitle("");
      setStagingNotice("Staged locally. Review the pending decision before Apple Calendar changes.");
    } catch (cause) {
      setStagingError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setStagingBusy(false);
    }
  }

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
      <section class="panel cal-connection">
        <div>
          <strong>Apple Calendar</strong>
          <p class="quiet">
            {app.runtime.calendarResources()?.detail ?? "Checking this profile's connection…"}
          </p>
        </div>
        <Show
          when={app.runtime.calendarResources()?.status === "ready"}
          fallback={
            <button
              class="small-action"
              disabled={connectionBusy() || app.runtime.calendarResources() === null}
              onClick={() => void setAppleConnection(true)}
            >
              Connect Apple Calendar
            </button>
          }
        >
          <button
            class="small-action"
            disabled={connectionBusy()}
            onClick={() => void setAppleConnection(false)}
          >
            Disconnect Apple Calendar
          </button>
        </Show>
        <Show when={connectionError()}>
          {(message) => <p class="surface-error">{message()}</p>}
        </Show>
      </section>

      <Show when={app.runtime.calendarResources()?.status === "ready"}>
        <form class="panel cal-stage" onSubmit={(event) => void stageAppleEvent(event)}>
          <header>
            <div>
              <strong>Stage an Apple event</strong>
              <p class="quiet">Creates a local hold first. Apple Calendar waits for approval.</p>
            </div>
          </header>
          <div class="cal-stage-fields">
            <label class="cal-stage-title">
              <span>Event title</span>
              <input
                value={title()}
                onInput={(event) => setTitle(event.currentTarget.value)}
                autocomplete="off"
                required
              />
            </label>
            <label>
              <span>Event date</span>
              <input
                type="date"
                value={date()}
                onInput={(event) => setDate(event.currentTarget.value)}
                required
              />
            </label>
            <label>
              <span>Event start</span>
              <input
                type="time"
                value={start()}
                onInput={(event) => setStart(event.currentTarget.value)}
                required
              />
            </label>
            <label>
              <span>Event end</span>
              <input
                type="time"
                value={end()}
                onInput={(event) => setEnd(event.currentTarget.value)}
                required
              />
            </label>
            <label>
              <span>Calendar</span>
              <select
                value={promotionCalendar()}
                onChange={(event) => setSelectedCalendar(event.currentTarget.value)}
                required
              >
                <For each={writableCalendars()}>
                  {(calendar) => <option value={calendar.name}>{calendar.name}</option>}
                </For>
              </select>
            </label>
          </div>
          <div class="cal-stage-actions">
            <Show when={stagingError()}>{(message) => <p class="surface-error">{message()}</p>}</Show>
            <Show when={stagingNotice()}>{(message) => <p class="quiet">{message()}</p>}</Show>
            <button class="btn-primary" type="submit" disabled={stagingBusy() || !promotionCalendar()}>
              Stage Apple event
            </button>
          </div>
        </form>
      </Show>

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
  refresh: (app) =>
    Promise.all([app.runtime.loadCalendar(), app.runtime.loadCalendarResources()]).then(
      () => undefined,
    ),
};
