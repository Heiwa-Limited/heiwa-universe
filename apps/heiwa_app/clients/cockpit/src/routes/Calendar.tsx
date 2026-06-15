import type { JSX } from "solid-js";
import { createSignal, For, Show } from "solid-js";
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
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  async function submit(event: Event): Promise<void> {
    event.preventDefault();
    if (!title().trim() || busy()) return;
    setBusy(true);
    setError(null);
    try {
      await v1.createHold({
        title: title().trim(),
        date: date() || undefined,
        start: start() || undefined,
        end: end() || undefined,
        kind: kind(),
      });
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
        <button
          type="submit"
          class="btn btn-outline"
          disabled={busy() || !title().trim()}
        >
          {busy() ? "Saving…" : "Add hold"}
        </button>
      </div>
      <Show when={error()}>
        {(message) => <p class="repl-error">{message()}</p>}
      </Show>
      <p class="muted">
        Holds stay local under <code>~/.heiwa/state/calendar</code> with a
        receipt; promotion to Apple/Google is approval-gated.
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
