import type { JSX } from "solid-js";
import {
  createEffect,
  createResource,
  createSignal,
  For,
  Show,
} from "solid-js";
import { v1 } from "../lib/endpoints";
import { RemoteShell } from "../lib/resource";
import type { CalendarSummary } from "../lib/types";
import { EmptyState, PageHero, Panel, StatusBadge } from "../lib/ui";

function HoldForm(props: { onCreated: () => void }): JSX.Element {
  const [title, setTitle] = createSignal("");
  const [date, setDate] = createSignal("");
  const [start, setStart] = createSignal("");
  const [end, setEnd] = createSignal("");
  const [kind, setKind] = createSignal("focus");
  const [destination, setDestination] = createSignal<"local" | "apple">(
    "local",
  );
  const [calendar, setCalendar] = createSignal("Calendar");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [notice, setNotice] = createSignal<string | null>(null);
  const [resources, { refetch: refetchResources }] = createResource(() =>
    v1.calendarResources(),
  );
  const [connectionBusy, setConnectionBusy] = createSignal(false);

  const writableCalendars = () =>
    (resources()?.calendars ?? []).filter((item) => item.writable);

  createEffect(() => {
    const calendars = writableCalendars();
    const first = calendars[0];
    if (first && !calendars.some((item) => item.name === calendar())) {
      setCalendar(first.name);
    }
  });

  const appleReady = () =>
    destination() !== "apple" ||
    (resources()?.status === "ready" &&
      writableCalendars().some((item) => item.name === calendar()) &&
      Boolean(date() && start() && end()));

  async function submit(event: Event): Promise<void> {
    event.preventDefault();
    if (!title().trim() || busy()) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const result = await v1.createHold({
        title: title().trim(),
        date: date() || undefined,
        start: start() || undefined,
        end: end() || undefined,
        kind: kind(),
        promotion:
          destination() === "apple"
            ? {
                connector: "apple_calendar",
                calendar: calendar(),
              }
            : undefined,
      });
      setNotice(
        result.approval_request
          ? `Staged for approval: ${result.approval_request.request_id}. The Apple event has not been created yet.`
          : "Local hold created.",
      );
      setTitle("");
      setStart("");
      setEnd("");
      props.onCreated();
    } catch (err) {
      const message =
        err instanceof Error
          ? err.message
          : ((err as { message?: string }).message ?? String(err));
      setError(message);
    } finally {
      setBusy(false);
    }
  }

  async function connectAppleCalendar(): Promise<void> {
    if (connectionBusy()) return;
    setConnectionBusy(true);
    setError(null);
    try {
      await v1.connectAppleCalendar();
      await refetchResources();
    } catch (err) {
      const message =
        err instanceof Error
          ? err.message
          : ((err as { message?: string }).message ?? String(err));
      setError(message);
    } finally {
      setConnectionBusy(false);
    }
  }

  return (
    <form class="hold-form" onSubmit={(event) => void submit(event)}>
      <div class="hold-form-row">
        <input
          type="text"
          placeholder="Hold title (e.g. Focus block)"
          value={title()}
          onInput={(event) => setTitle(event.currentTarget.value)}
        />
        <input
          type="date"
          value={date()}
          onInput={(event) => setDate(event.currentTarget.value)}
          aria-label="Date"
        />
        <input
          type="time"
          value={start()}
          onInput={(event) => setStart(event.currentTarget.value)}
          aria-label="Start time"
        />
        <input
          type="time"
          value={end()}
          onInput={(event) => setEnd(event.currentTarget.value)}
          aria-label="End time"
        />
        <select
          value={kind()}
          onInput={(event) => setKind(event.currentTarget.value)}
          aria-label="Hold kind"
        >
          <option value="focus">focus</option>
          <option value="travel">travel</option>
          <option value="soft">soft</option>
        </select>
        <select
          value={destination()}
          onInput={(event) =>
            setDestination(event.currentTarget.value as "local" | "apple")
          }
          aria-label="Hold destination"
        >
          <option value="local">Local hold</option>
          <option value="apple">Stage for Apple Calendar</option>
        </select>
        <Show when={destination() === "apple"}>
          <select
            value={calendar()}
            onInput={(event) => setCalendar(event.currentTarget.value)}
            aria-label="Apple calendar"
            disabled={resources.loading || writableCalendars().length === 0}
          >
            <Show when={writableCalendars().length === 0}>
              <option value="">No writable calendars detected</option>
            </Show>
            <For each={writableCalendars()}>
              {(item) => <option value={item.name}>{item.name}</option>}
            </For>
          </select>
        </Show>
        <button
          type="submit"
          class="btn btn-outline"
          disabled={busy() || !title().trim() || !appleReady()}
        >
          {busy()
            ? "Saving…"
            : destination() === "apple"
              ? "Stage event"
              : "Add hold"}
        </button>
      </div>
      <Show when={error()}>
        {(message) => <p class="repl-error">{message()}</p>}
      </Show>
      <Show when={notice()}>
        {(message) => <p class="muted">{message()}</p>}
      </Show>
      <Show when={destination() === "apple" && resources()?.status === "error"}>
        <p class="repl-error">
          {resources()?.error ?? "Apple Calendar resources are unavailable."}
        </p>
      </Show>
      <Show
        when={
          destination() === "apple" &&
          resources() !== undefined &&
          resources()?.status !== "ready"
        }
      >
        <div class="hero-actions">
          <button
            class="btn btn-outline"
            type="button"
            disabled={connectionBusy()}
            onClick={() => void connectAppleCalendar()}
          >
            Connect Apple Calendar
          </button>
          <span class="muted">
            {resources()?.detail ??
              "This Heiwa profile has not enrolled Apple Calendar."}
          </span>
        </div>
      </Show>
      <p class="muted">
        Local holds stay under <code>~/.heiwa/state/calendar</code>. Apple
        events are staged as T2 approvals; nothing is written to Calendar.app
        until you approve the exact target and time.
      </p>
    </form>
  );
}

function TodayPanel(props: {
  data: CalendarSummary;
  refetch: () => void;
}): JSX.Element {
  return (
    <Panel
      title={`Today pressure · ${props.data.date}`}
      status={`${props.data.counts.moments_today} moments`}
      tone="ok"
    >
      <Show
        when={props.data.today.length > 0}
        fallback={
          <p class="muted">
            No holds or dated appointments today. Add a hold below or sync an
            external calendar lane.
          </p>
        }
      >
        <ul>
          <For each={props.data.today}>
            {(moment) => (
              <li>
                <code>{moment.time}</code> <strong>{moment.title}</strong>{" "}
                <StatusBadge status={moment.pressure} />
                <p class="muted">
                  {moment.source} · {moment.detail}
                </p>
              </li>
            )}
          </For>
        </ul>
      </Show>
      <HoldForm onCreated={props.refetch} />
    </Panel>
  );
}

export default function CalendarRoute(): JSX.Element {
  return (
    <section>
      <PageHero
        eyebrow="Calendar"
        title="Heiwa Calendar"
        lede="One local commitment map for Apple Calendar, Google Calendar, and Heiwa-owned holds. Syncs become read models first; external writes are staged through approvals and receipts."
      />

      <RemoteShell loader={() => v1.calendarSummary()}>
        {(data, refetch) => (
          <>
            <div class="panels">
              <TodayPanel data={data} refetch={refetch} />

              <Panel
                title="Holds"
                status={`${data.counts.holds_total} total`}
                tone="ok"
              >
                <Show
                  when={data.holds.length > 0}
                  fallback={
                    <EmptyState title="No holds yet.">
                      <p class="muted">
                        Create one here or via{" "}
                        <code>heiwa calendar hold add</code>.
                      </p>
                    </EmptyState>
                  }
                >
                  <ul>
                    <For each={data.holds}>
                      {(hold) => (
                        <li>
                          <code>
                            {hold.date} {hold.start ?? "--:--"}
                            {hold.end ? `–${hold.end}` : ""}
                          </code>{" "}
                          <strong>{hold.title}</strong>{" "}
                          <StatusBadge status={hold.kind} tone="ok" />{" "}
                          <StatusBadge status={hold.status} />
                          <Show when={hold.note}>
                            <p class="muted">{hold.note}</p>
                          </Show>
                        </li>
                      )}
                    </For>
                  </ul>
                </Show>
              </Panel>
            </div>

            <div class="card-list">
              <For each={data.lanes}>
                {(lane) => (
                  <article>
                    <div class="status-card-head">
                      <h3>{lane.name}</h3>
                      <StatusBadge status={lane.status} />
                    </div>
                    <p>
                      <strong>Sync:</strong> {lane.sync}
                    </p>
                    <p>
                      <strong>Writes:</strong> {lane.write}
                    </p>
                    <p class="muted">
                      <strong>Evidence:</strong> {lane.evidence}
                    </p>
                  </article>
                )}
              </For>
            </div>
          </>
        )}
      </RemoteShell>
    </section>
  );
}
