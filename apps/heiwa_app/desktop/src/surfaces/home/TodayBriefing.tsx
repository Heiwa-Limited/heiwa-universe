import { For, Show } from "solid-js";
import { localIsoDate } from "../../lib/format";
import { useApp } from "../../state/app";
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
  // has to mean the day the machine is having.
  const todayIso = () => localIsoDate();

  const todaysEvents = () =>
    app.runtime
      .calendarEvents()
      .filter((event) => event.date === todayIso())
      // Chronological: a briefing is read top to bottom as the day runs.
      .sort((left, right) => (left.start ?? "").localeCompare(right.start ?? ""));

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
        fallback={<p class="today-clear">Nothing scheduled today.</p>}
      >
        <ul class="today-events">
          <For each={todaysEvents()}>
            {(event) => (
              <li class="today-event">
                <span class="today-time">{event.start || "--:--"}</span>
                <span class="today-event-title">{event.title}</span>
              </li>
            )}
          </For>
        </ul>
      </Show>
    </section>
  );
}
