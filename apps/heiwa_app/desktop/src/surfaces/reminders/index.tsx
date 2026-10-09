import { For, Show, createSignal, onMount } from "solid-js";
import { useApp } from "../../state/app";
import type { ReminderList, ReminderRead, ReminderRow, ReminderStatus } from "../../state/types";
import type { SurfaceModule } from "../types";
import "../mail/mail.css";

function dueLabel(due: ReminderRow["due"]): string {
  if (!due) return "No due date";
  if (due.date) return due.date;
  if (due.instant) {
    const date = new Date(due.instant);
    if (!Number.isNaN(date.getTime())) {
      return new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" }).format(date);
    }
  }
  return "Due date unavailable";
}

const message = (cause: unknown, fallback: string) =>
  cause instanceof Error ? cause.message : typeof cause === "string" ? cause : fallback;

/**
 * Apple Reminders, read only. Connecting from here (not a terminal) is what
 * makes macOS ask for Reminders access on Heiwa's behalf.
 */
function Reminders() {
  const app = useApp();
  const [status, setStatus] = createSignal<ReminderStatus>();
  const [lists, setLists] = createSignal<ReminderList[]>([]);
  const [chosen, setChosen] = createSignal<string[]>([]);
  const [read, setRead] = createSignal<ReminderRead>();
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string>();

  const run = async (action: () => Promise<void>, fallback: string) => {
    if (busy()) return;
    setBusy(true);
    setError(undefined);
    try { await action(); } catch (cause) { setError(message(cause, fallback)); } finally { setBusy(false); }
  };

  const refresh = async () => {
    const current = await app.runtime.reminderStatus();
    setStatus(current);
    if (current.status !== "connected") return;
    const inventory = await app.runtime.reminderLists();
    setLists(inventory.lists);
    const saved = current.selected_list_ids ?? [];
    setChosen(saved);
    if (saved.length) setRead(await app.runtime.readReminders());
  };

  onMount(() => void run(refresh, "Apple Reminders could not be checked."));

  const connect = () => run(async () => {
    await app.runtime.connectReminders();
    await refresh();
  }, "Apple Reminders could not be connected. Check System Settings › Privacy & Security › Reminders.");

  const save = () => run(async () => {
    await app.runtime.selectReminderLists(chosen());
    setRead(await app.runtime.readReminders());
  }, "The selected Reminders lists could not be read.");

  const toggle = (id: string, on: boolean) =>
    setChosen(on ? [...chosen().filter((item) => item !== id), id] : chosen().filter((item) => item !== id));
  const listName = (id: string) => lists().find((list) => list.id === id)?.name ?? "Reminders";

  return <section class="mail-surface" aria-label="Reminders">
    <header class="mail-header">
      <div>
        <h2 class="mail-title">Reminders</h2>
        <p class="mail-subtitle">Read only · lists you choose · nothing leaves this Mac</p>
      </div>
      <Show when={status() && status()!.status !== "connected"}>
        <button class="mail-read" disabled={busy()} onClick={() => void connect()}>
          {busy() ? "Connecting…" : "Connect Apple Reminders"}
        </button>
      </Show>
    </header>
    <Show when={error()}><p class="mail-error" role="alert">{error()}</p></Show>
    <Show when={status() && status()!.status !== "connected"}>
      <p class="mail-policy">macOS will ask whether Heiwa may access Reminders. Heiwa reads only the lists you choose and never changes your reminders.</p>
    </Show>

    <Show when={status()?.status === "connected"}>
      <fieldset class="mail-policy" aria-label="Reminders lists to read">
        <legend>Lists to read</legend>
        <For each={lists()}>{(list) => <label style={{ display: "block" }}>
          <input type="checkbox" checked={chosen().includes(list.id)}
            onChange={(event) => toggle(list.id, event.currentTarget.checked)} />
          {" "}{list.name} <small>· {list.source}{list.writable ? "" : " · read-only"}</small>
        </label>}</For>
        <button class="mail-read" disabled={busy() || !chosen().length} onClick={() => void save()}>
          {busy() ? "Reading…" : "Save and read lists"}
        </button>
      </fieldset>
    </Show>

    <Show when={read()}>
      <Show when={!read()!.complete}>
        <p class="mail-result" role="status">Some reminders were not read; a missing reminder may still exist.</p>
      </Show>
      <Show when={read()!.reminders.length > 0} fallback={<div class="mail-empty"><p>No reminders in the selected lists.</p></div>}>
        <ul class="mail-list">
          <For each={read()!.reminders}>{(row) => <li class="mail-row" classList={{ "mail-row-unread": !row.completed }}>
            <span class="mail-sender">{row.title}<small class="mail-account">{listName(row.list_id)}</small></span>
            <span class="mail-subject">{dueLabel(row.due)}</span>
            <span class="mail-read-state">{row.completed ? "Done" : "Open"}</span>
          </li>}</For>
        </ul>
      </Show>
    </Show>
  </section>;
}

export const remindersSurface: SurfaceModule = {
  id: "reminders",
  label: "Reminders",
  glyph: "☑",
  caption: "reminders window",
  Component: Reminders,
  preview: () => ({ title: "Reminders", lines: ["read only", "lists you choose"] }),
};
