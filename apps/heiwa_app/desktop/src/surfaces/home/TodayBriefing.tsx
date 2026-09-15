import { createMemo, For, Show } from "solid-js";
import { localIsoDate } from "../../lib/format";
import { useApp } from "../../state/app";
import { agendaTimeLabel, normalizeEvents, occursOn } from "../../state/calendar-model";
import "./today-briefing.css";

/**
 * What today holds, answered on open.
 *
 * The calendar and the mail are both on this machine already, which means the
 * app can answer "what do I need to know right now" without the user going
 * and looking in two places. That is the whole difference between a set of
 * surfaces and an assistant.
 *
 * Deliberately narrow: today's events and unread count, nothing else. A
 * briefing that lists next week is a list, and a list is what the user was
 * trying to avoid reading.
 */
export function TodayBriefing() {
  const app = useApp();

  // Local, not UTC: the events came off this machine's calendar, so "today"
  // has to mean the day the machine is having. The shared calendar model owns
  // that reading, including events that started yesterday and run into today.
  const today = () => localIsoDate();
  const todaysEvents = createMemo(() =>
    normalizeEvents(app.runtime.calendarEvents()).filter((event) => occursOn(event, today())));

  const unread = () => app.runtime.mail().filter((message) => message.unread).length;

  return (
    <section class="today-briefing" aria-label="Today">
      <header class="today-head">
        <h2 class="today-title">Today</h2>
        <Show when={unread() > 0}>
          <span class="today-unread">{unread()} unread</span>
        </Show>
      </header>

      <Show
        when={todaysEvents().length > 0}
        fallback={<p class="today-clear">No calendar items loaded for today.</p>}
      >
        <ul class="today-events">
          <For each={todaysEvents()}>
            {(event) => (
              <li class="today-event" classList={{ cancelled: event.status === "cancelled" }}>
                <button
                  type="button"
                  class="today-event-open"
                  onClick={() => {
                    app.runtime.focusCalendarEvent(event.id);
                    app.navigate("calendar");
                  }}
                >
                  <span class="today-time">{agendaTimeLabel(event, today())}</span>
                  <span class="today-event-title">{event.title}</span>
                </button>
              </li>
            )}
          </For>
        </ul>
      </Show>
    </section>
  );
}
