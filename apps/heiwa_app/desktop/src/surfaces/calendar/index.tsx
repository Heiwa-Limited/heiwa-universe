import { createEffect, createMemo, createSignal, For, on, onCleanup, Show, untrack } from "solid-js";
import { localIsoDate, parseLocalDate } from "../../lib/format";
import { useApp } from "../../state/app";
import {
  addIsoDays,
  ageLabel,
  agendaTimeLabel,
  calendarSeries,
  dayLabel,
  durationLabel,
  groupByDay,
  indexByDay,
  longDayLabel,
  monthGrid,
  normalizeEvents,
  occursOn,
  relativeLabel,
  sourceLabel,
  timeLabel,
  timeRange,
  upcomingEvents,
  type AgendaEvent,
} from "../../state/calendar-model";
import type { SurfaceModule } from "../types";
import "./calendar.css";

const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
/** How far ahead "Upcoming" looks. */
const UPCOMING_DAYS = 14;
/** Upcoming rows shown at first, and added by each "Show more". */
const UPCOMING_PAGE = 40;
/** How often the open Calendar asks the runtime whether its copy is stale. */
const LIVE_INTERVAL_MS = 60_000;

function countLabel(count: number): string {
  if (count === 0) return "no events";
  return count === 1 ? "1 event" : `${count} events`;
}

/** Points the calendar's color at its theme series token. */
function seriesStyle(event: AgendaEvent): Record<string, string> {
  return { "--cal-series": `var(--series-${calendarSeries(event.calendar)})` };
}

function CalendarSurface() {
  const app = useApp();
  // Relative labels ("in 5 min", "Updated 2 min ago") and the upcoming cut
  // are all measured against this, so it has to move while the view is open.
  const [now, setNow] = createSignal(new Date());
  const clockTimer = setInterval(() => setNow(new Date()), 30_000);
  onCleanup(() => clearInterval(clockTimer));
  const today = () => localIsoDate(now());

  // Surface-local view state: no other surface needs the month or selection.
  const [cursor, setCursor] = createSignal(new Date());
  const [selectedDay, setSelectedDay] = createSignal<string>();
  const [expanded, setExpanded] = createSignal<string>();

  const grid = createMemo(() => monthGrid(cursor(), today()));
  const monthStart = () => localIsoDate(new Date(cursor().getFullYear(), cursor().getMonth(), 1));
  const monthEnd = () => localIsoDate(new Date(cursor().getFullYear(), cursor().getMonth() + 1, 0));
  const monthName = () => cursor().toLocaleDateString([], { month: "long", year: "numeric" });
  const viewingThisMonth = () => today() >= monthStart() && today() <= monthEnd();

  // Load what is on screen: the grid's weeks, widened to cover the upcoming
  // list whenever this month holds today.
  const range = createMemo(
    () => {
      const { from, to } = grid();
      if (!viewingThisMonth()) return { from, to };
      const aheadFrom = addIsoDays(today(), -1);
      const aheadTo = addIsoDays(today(), UPCOMING_DAYS + 1);
      return { from: aheadFrom < from ? aheadFrom : from, to: aheadTo > to ? aheadTo : to };
    },
    undefined,
    { equals: (left, right) => left.from === right.from && left.to === right.to },
  );
  createEffect(on(range, (next) => void app.runtime.loadCalendar(next)));

  // Seeded with the previous result so unchanged events keep their identity
  // and their rows survive the live reload.
  const events = createMemo<AgendaEvent[]>(
    (previous) => normalizeEvents(app.runtime.calendarEvents(), previous),
    [],
  );
  const byDay = createMemo(() => indexByDay(events(), grid().from, grid().to));

  const agendaMode = (): "day" | "upcoming" | "month" =>
    selectedDay() ? "day" : viewingThisMonth() ? "upcoming" : "month";
  const agendaTitle = () => {
    const day = selectedDay();
    if (day) return longDayLabel(day);
    return viewingThisMonth() ? "Upcoming" : monthName();
  };
  const [upcomingShown, setUpcomingShown] = createSignal(UPCOMING_PAGE);
  const upcoming = createMemo(() =>
    upcomingEvents(events(), now(), { days: UPCOMING_DAYS, limit: Number.POSITIVE_INFINITY }));
  const agendaGroups = createMemo(() => {
    const day = selectedDay();
    if (day) return [{ day, events: events().filter((event) => occursOn(event, day)) }];
    if (viewingThisMonth()) return groupByDay(upcoming().slice(0, upcomingShown()), today());
    const inMonth = events().filter((event) => event.endDay >= monthStart() && event.startDay <= monthEnd());
    return groupByDay(inMonth, monthStart());
  });
  // Keyed by day string rather than group object, so a day's section is
  // reused when its contents are recomputed.
  const agendaDays = createMemo(
    () => agendaGroups().map((group) => group.day),
    [],
    { equals: (left, right) => left.length === right.length && left.every((day, index) => day === right[index]) },
  );
  const agendaByDay = createMemo(() => new Map(agendaGroups().map((group) => [group.day, group.events])));
  const agendaCount = () => agendaGroups().reduce((total, group) => total + group.events.length, 0);
  /** Everything the agenda covers, including upcoming rows not shown yet. */
  const agendaTotal = () => (agendaMode() === "upcoming" ? upcoming().length : agendaCount());
  const emptyAgenda = () => {
    if (agendaMode() === "day") return "Nothing on this day.";
    if (agendaMode() === "month") return "Nothing on the calendar this month.";
    return app.runtime.calendarSync()?.status === "no_selection"
      ? "Choose calendars below to see your schedule here."
      : "Nothing scheduled in the next two weeks.";
  };

  function showMonthOf(day: string): void {
    const date = parseLocalDate(day);
    if (!date) return;
    if (date.getFullYear() !== cursor().getFullYear() || date.getMonth() !== cursor().getMonth()) {
      setCursor(new Date(date.getFullYear(), date.getMonth(), 1));
    }
  }

  function shiftMonth(offset: number): void {
    setSelectedDay(undefined);
    setCursor((date) => new Date(date.getFullYear(), date.getMonth() + offset, 1));
  }

  function pickDay(day: string): void {
    if (selectedDay() === day) {
      setSelectedDay(undefined);
      return;
    }
    showMonthOf(day);
    setSelectedDay(day);
  }

  function showDay(event: AgendaEvent): void {
    showMonthOf(event.startDay);
    setSelectedDay(event.startDay);
    setExpanded(event.id);
  }

  // Another surface (Home's briefing) can ask for an event to be opened.
  createEffect(() => {
    const id = app.runtime.calendarFocus();
    const loaded = events();
    if (!id || !loaded.length) return;
    untrack(() => {
      app.runtime.focusCalendarEvent(undefined);
      const event = loaded.find((candidate) => candidate.id === id);
      if (!event) return;
      showMonthOf(event.startDay);
      const inUpcoming = event.end > now() && event.startDay <= addIsoDays(today(), UPCOMING_DAYS);
      setSelectedDay(inUpcoming && viewingThisMonth() ? undefined : event.startDay);
      setExpanded(event.id);
    });
  });

  const ready = () => app.runtime.calendarResources()?.status === "ready";
  const syncText = () => {
    if (app.runtime.calendarSyncing()) return "Syncing…";
    const sync = app.runtime.calendarSync();
    if (!sync) return "";
    switch (sync.status) {
      case "error":
        return sync.error ? `Sync failed: ${sync.error}` : "Sync failed";
      case "no_selection":
        return "Choose calendars to sync";
      case "not_connected":
        return "Not connected";
      case "never":
        return "Not synced yet";
      default: {
        const read = sync.last_read_at ? new Date(sync.last_read_at) : null;
        const age = read && !Number.isNaN(read.getTime()) ? `Updated ${ageLabel(read, now())}` : "Up to date";
        return sync.complete === false ? `${age} · partial` : age;
      }
    }
  };

  return (
    <div class="view calendar-view">
      <Show when={!ready()}>
        <ConnectionPanel />
      </Show>

      <div class="cal-toolbar">
        <div class="cal-nav">
          <button class="small-action" aria-label="Previous month" onClick={() => shiftMonth(-1)}>‹</button>
          <h2>{monthName()}</h2>
          <button class="small-action" aria-label="Next month" onClick={() => shiftMonth(1)}>›</button>
          <button
            class="small-action"
            onClick={() => {
              setSelectedDay(undefined);
              setCursor(new Date());
            }}
          >
            Today
          </button>
        </div>
        <Show when={ready()}>
          <div class="cal-sync">
            <span
              role="status"
              aria-label="Calendar sync"
              class="cal-sync-status"
              classList={{ error: app.runtime.calendarSync()?.status === "error" }}
              title={app.runtime.calendarSync()?.complete === false
                ? "Part of the date range could not be read, so events Heiwa did not see were kept."
                : undefined}
            >
              {syncText()}
            </span>
            <button
              class="small-action"
              disabled={app.runtime.calendarSyncing()}
              onClick={() => void app.runtime.syncCalendar({ force: true })}
            >
              Sync now
            </button>
          </div>
        </Show>
      </div>
      <Show when={app.runtime.calendarError()}>
        {(message) => <p class="surface-error">{message()} Showing the last loaded calendar.</p>}
      </Show>

      <div class="cal-body">
        <div class="cal-grid" role="table" aria-label={monthName()}>
          <div class="cal-grid-row" role="row">
            <For each={WEEKDAYS}>{(day) => <div class="cal-weekday" role="columnheader">{day}</div>}</For>
          </div>
          <For each={grid().weeks}>
            {(week) => (
              <div class="cal-grid-row" role="row">
                <For each={week}>
                  {(cell) => {
                    const dayEvents = () => byDay().get(cell.iso) ?? [];
                    return (
                      <div class="cal-cell" role="cell">
                        <button
                          type="button"
                          class="cal-day-button"
                          classList={{
                            outside: !cell.inMonth,
                            today: cell.isToday,
                            selected: selectedDay() === cell.iso,
                          }}
                          aria-pressed={selectedDay() === cell.iso}
                          aria-label={`${longDayLabel(cell.iso)}${cell.isToday ? ", today" : ""}, ${countLabel(dayEvents().length)}`}
                          onClick={() => pickDay(cell.iso)}
                        >
                          <span class="cal-day">{cell.day}</span>
                          <span class="cal-dots" aria-hidden="true">
                            <For each={dayEvents().slice(0, 3)}>
                              {(event) => <span class="cal-dot" style={seriesStyle(event)} />}
                            </For>
                            <Show when={dayEvents().length > 3}>
                              <span class="cal-more">+{dayEvents().length - 3}</span>
                            </Show>
                          </span>
                        </button>
                      </div>
                    );
                  }}
                </For>
              </div>
            )}
          </For>
        </div>

        <section class="panel cal-agenda" aria-label={`${agendaTitle()} agenda`}>
          <header>
            <h3>{agendaTitle()}</h3>
            <strong aria-label={`${agendaTotal()} ${agendaTotal() === 1 ? "event" : "events"}`}>{agendaTotal()}</strong>
          </header>
          <Show when={selectedDay()}>
            <button class="small-action cal-agenda-back" onClick={() => setSelectedDay(undefined)}>
              {viewingThisMonth() ? "Back to upcoming" : `Back to ${monthName()}`}
            </button>
          </Show>
          <Show when={agendaCount() > 0} fallback={<div class="empty-state">{emptyAgenda()}</div>}>
            <For each={agendaDays()}>
              {(day) => (
                <div class="cal-agenda-group">
                  <Show when={agendaMode() !== "day"}>
                    <h4 class="cal-agenda-day">{dayLabel(day, today())}</h4>
                  </Show>
                  <ul class="cal-agenda-list">
                    <For each={agendaByDay().get(day) ?? []}>
                      {(event) => (
                        <AgendaRow
                          event={event}
                          day={day}
                          now={now()}
                          expanded={expanded() === event.id}
                          onToggle={() => setExpanded((current) => (current === event.id ? undefined : event.id))}
                          onShowDay={() => showDay(event)}
                          onReview={() => app.navigate("approvals")}
                        />
                      )}
                    </For>
                  </ul>
                </div>
              )}
            </For>
            <Show when={agendaMode() === "upcoming" && upcoming().length > upcomingShown()}>
              <button class="small-action cal-agenda-more" onClick={() => setUpcomingShown((shown) => shown + UPCOMING_PAGE)}>
                Show more ({upcoming().length - upcomingShown()} left)
              </button>
            </Show>
          </Show>
        </section>
      </div>

      <Show when={ready()}>
        <CalendarSettings />
        <StageEventForm />
      </Show>
    </div>
  );
}

function AgendaRow(props: {
  event: AgendaEvent;
  /** The day this row is listed under. */
  day: string;
  now: Date;
  expanded: boolean;
  onToggle: () => void;
  onShowDay: () => void;
  onReview: () => void;
}) {
  let row: HTMLButtonElement | undefined;
  const detailId = () => `cal-detail-${props.event.id.replace(/[^A-Za-z0-9_-]/g, "_")}`;
  const zone = Intl.DateTimeFormat().resolvedOptions().timeZone;
  const status = () => {
    if (props.event.status === "cancelled") return "Cancelled";
    if (props.event.status === "draft") return "Draft — waiting for your approval";
    return props.event.status === "confirmed" ? "" : props.event.status;
  };
  const when = () =>
    props.event.startDay === props.event.endDay
      ? longDayLabel(props.event.startDay)
      : `${longDayLabel(props.event.startDay)} – ${longDayLabel(props.event.endDay)}`;

  return (
    <li
      class="cal-agenda-item"
      classList={{ expanded: props.expanded, cancelled: props.event.status === "cancelled" }}
      style={seriesStyle(props.event)}
    >
      <button
        ref={row}
        type="button"
        class="cal-agenda-row"
        aria-expanded={props.expanded}
        aria-controls={detailId()}
        onClick={() => props.onToggle()}
      >
        <span class="cal-agenda-time">{agendaTimeLabel(props.event, props.day)}</span>
        <span class="cal-agenda-swatch" aria-hidden="true" />
        <span class="cal-agenda-text">
          <span class="cal-agenda-title">{props.event.title}</span>
          <span class="cal-agenda-meta">
            {props.event.calendar}
            {props.event.startDay !== props.event.endDay
              ? ` · ${timeRange(props.event)}`
              : props.event.allDay ? "" : ` · ${durationLabel(props.event)}`}
          </span>
        </span>
        <Show when={relativeLabel(props.event, props.now)}>
          {(label) => <span class="cal-agenda-badge">{label()}</span>}
        </Show>
      </button>
      <Show when={props.expanded}>
        <div
          id={detailId()}
          class="cal-event-detail"
          role="region"
          aria-label={`${props.event.title} details`}
          onKeyDown={(event) => {
            if (event.key !== "Escape") return;
            event.stopPropagation();
            props.onToggle();
            row?.focus();
          }}
        >
          <dl>
            <div>
              <dt>When</dt>
              <dd>
                {when()}
                <br />
                {timeRange(props.event)} · {durationLabel(props.event)}
                {props.event.allDay ? "" : ` · ${zone}`}
              </dd>
            </div>
            <div>
              <dt>Calendar</dt>
              <dd><span class="cal-agenda-swatch" aria-hidden="true" /> {props.event.calendar}</dd>
            </div>
            <div>
              <dt>From</dt>
              <dd>{sourceLabel(props.event.source, props.event.kind)}</dd>
            </div>
            <Show when={props.event.recurring}>
              <div><dt>Repeats</dt><dd>Recurring event</dd></div>
            </Show>
            <Show when={status()}>
              <div><dt>Status</dt><dd>{status()}</dd></div>
            </Show>
            <Show when={props.event.note}>
              <div><dt>Note</dt><dd>{props.event.note}</dd></div>
            </Show>
          </dl>
          <div class="cal-event-actions">
            <button type="button" class="small-action" onClick={() => props.onShowDay()}>Show day</button>
            <Show when={props.event.status === "draft"}>
              <button type="button" class="small-action" onClick={() => props.onReview()}>Review pending changes</button>
            </Show>
          </div>
        </div>
      </Show>
    </li>
  );
}

function ConnectionPanel() {
  const app = useApp();
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  async function connect(): Promise<void> {
    if (busy()) return;
    setBusy(true);
    setError(null);
    try {
      await app.runtime.connectAppleCalendar();
      await app.runtime.syncCalendar({ force: true });
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section class="panel cal-connection">
      <div>
        <strong>Apple Calendar</strong>
        <p class="quiet">
          {app.runtime.calendarResources()?.detail ?? "Checking this profile's connection…"}
        </p>
      </div>
      <button
        class="small-action"
        disabled={busy() || app.runtime.calendarResources() === null}
        onClick={() => void connect()}
      >
        Connect Apple Calendar
      </button>
      <Show when={error()}>{(message) => <p class="surface-error">{message()}</p>}</Show>
    </section>
  );
}

function CalendarSettings() {
  const app = useApp();
  const [selection, setSelection] = createSignal<string[] | null>(null);
  const [readBusy, setReadBusy] = createSignal(false);
  const [notice, setNotice] = createSignal<string | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [disconnecting, setDisconnecting] = createSignal(false);
  const saved = () => app.runtime.calendarResources()?.selected_ids ?? [];
  const selectedIds = () => selection() ?? saved();
  const calendars = () => app.runtime.calendarResources()?.calendars.filter((calendar) => calendar.id) ?? [];

  async function syncSelected(): Promise<void> {
    if (readBusy() || !selectedIds().length) return;
    setReadBusy(true);
    setError(null);
    setNotice(null);
    try {
      const ids = selectedIds();
      const result = await app.runtime.readAppleCalendars(ids);
      setSelection(null);
      const from = ids.length === 1 ? "1 calendar" : `${ids.length} calendars`;
      setNotice(result.truncated
        ? `${result.fetched} events synced from ${from}. Part of the range could not be read, so events Heiwa did not see were kept.`
        : `${result.fetched} events synced from ${from}. Heiwa keeps them current while it runs.`);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setReadBusy(false);
    }
  }

  async function disconnect(): Promise<void> {
    if (disconnecting()) return;
    setDisconnecting(true);
    setError(null);
    try {
      await app.runtime.disconnectAppleCalendar();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setDisconnecting(false);
    }
  }

  return (
    // Open at first so a new connection lands on the choice it needs; once
    // calendars are chosen this is settings, and stays out of the way.
    <details class="panel cal-settings" open={untrack(() => saved().length === 0)}>
      <summary>
        <strong>Calendars</strong>
        <span class="quiet">
          {saved().length ? `${saved().length} synced` : "Choose what to sync"}
        </span>
      </summary>
      <Show
        when={app.runtime.calendarResources()?.reader_available}
        fallback={<p class="quiet">{app.runtime.calendarResources()?.detail}</p>}
      >
        <div class="cal-import" aria-busy={readBusy()}>
          <p class="quiet">
            Heiwa reads events from the last 31 days through the next 90, including repeats, and
            re-reads them while it runs. Nothing leaves this Mac.
          </p>
          <fieldset disabled={readBusy()}>
            <legend>Calendars to sync</legend>
            <For each={calendars()}>
              {(calendar) => (
                <label class="cal-resource-choice">
                  <input
                    type="checkbox"
                    checked={selectedIds().includes(calendar.id!)}
                    onChange={(event) =>
                      setSelection(event.currentTarget.checked
                        ? [...selectedIds().filter((id) => id !== calendar.id), calendar.id!]
                        : selectedIds().filter((id) => id !== calendar.id))}
                  />
                  <span>{calendar.name}{calendar.source ? ` · ${calendar.source}` : ""}</span>
                </label>
              )}
            </For>
          </fieldset>
          <button class="btn-primary" disabled={readBusy() || !selectedIds().length} onClick={() => void syncSelected()}>
            {readBusy() ? "Syncing calendars…" : "Sync selected calendars"}
          </button>
        </div>
      </Show>
      <Show when={notice()}><p class="quiet" role="status">{notice()}</p></Show>
      <Show when={error()}>{(message) => <p class="surface-error">{message()}</p>}</Show>
      <div class="cal-settings-footer">
        <button class="small-action" disabled={disconnecting() || readBusy()} onClick={() => void disconnect()}>
          Disconnect Apple Calendar
        </button>
      </div>
    </details>
  );
}

function StageEventForm() {
  const app = useApp();
  const [title, setTitle] = createSignal("");
  const [date, setDate] = createSignal("");
  const [start, setStart] = createSignal("");
  const [end, setEnd] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [notice, setNotice] = createSignal<string | null>(null);
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

  async function stage(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (busy()) return;
    const calendar = promotionCalendar();
    if (!title().trim() || !date() || !start() || !end() || !calendar) {
      setError("Add a title, date, start, end, and writable calendar.");
      return;
    }
    setBusy(true);
    setError(null);
    setNotice(null);
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
      setNotice("Staged locally. Review the pending decision before Apple Calendar changes.");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  }

  return (
    <form class="panel cal-stage" onSubmit={(event) => void stage(event)}>
      <header>
        <div>
          <strong>Stage an Apple event</strong>
          <p class="quiet">Creates a local hold first. Apple Calendar waits for approval.</p>
        </div>
      </header>
      <div class="cal-stage-fields">
        <label class="cal-stage-title">
          <span>Event title</span>
          <input value={title()} onInput={(event) => setTitle(event.currentTarget.value)} autocomplete="off" required />
        </label>
        <label>
          <span>Event date</span>
          <input type="date" value={date()} onInput={(event) => setDate(event.currentTarget.value)} required />
        </label>
        <label>
          <span>Event start</span>
          <input type="time" value={start()} onInput={(event) => setStart(event.currentTarget.value)} required />
        </label>
        <label>
          <span>Event end</span>
          <input type="time" value={end()} onInput={(event) => setEnd(event.currentTarget.value)} required />
        </label>
        <label>
          <span>Calendar</span>
          <select value={promotionCalendar()} onChange={(event) => setSelectedCalendar(event.currentTarget.value)} required>
            <For each={writableCalendars()}>
              {(calendar) => <option value={calendar.name}>{calendar.name}</option>}
            </For>
          </select>
        </label>
      </div>
      <div class="cal-stage-actions">
        <Show when={error()}>{(message) => <p class="surface-error">{message()}</p>}</Show>
        <Show when={notice()}>{(message) => <p class="quiet">{message()}</p>}</Show>
        <Show when={notice() || (app.runtime.approvals()?.pending?.length ?? 0) > 0}>
          <button type="button" class="small-action" onClick={() => app.navigate("approvals")}>Review pending changes</button>
        </Show>
        <button class="btn-primary" type="submit" disabled={busy() || !promotionCalendar()}>
          Stage Apple event
        </button>
      </div>
    </form>
  );
}

export const calendarSurface: SurfaceModule = {
  id: "calendar",
  label: "Calendar",
  glyph: "◷",
  caption: "calendar window",
  Component: CalendarSurface,
  liveIntervalMs: LIVE_INTERVAL_MS,
  preview: (app) => {
    const now = new Date();
    const todayIso = localIsoDate(now);
    const events = normalizeEvents(app.runtime.calendarEvents());
    const next = upcomingEvents(events, now, { days: 7, limit: 1 })[0];
    const todayCount = events.filter((event) => occursOn(event, todayIso)).length;
    return {
      title: "Calendar",
      lines: [
        next
          ? `${dayLabel(next.startDay < todayIso ? todayIso : next.startDay, todayIso)} ${timeLabel(next)} · ${next.title}`
          : "nothing coming up",
        `${todayCount} today`,
      ],
    };
  },
  // Sync first-class: the runtime re-reads EventKit only when its copy is
  // stale, so this is cheap to run on arrival, focus, and the live interval.
  refresh: (app) =>
    Promise.all([
      app.runtime.syncCalendar(),
      app.runtime.loadCalendar(),
      app.runtime.loadCalendarResources(),
      app.runtime.loadApprovals(),
    ]).then(() => undefined),
};
