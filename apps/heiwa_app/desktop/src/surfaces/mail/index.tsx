import { For, Show, createSignal } from "solid-js";
import { useApp } from "../../state/app";
import type { SurfaceModule } from "../types";
import "./mail.css";

function displayDate(value: string | undefined): string {
  if (!value) return "Date unavailable";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "Date unavailable";
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(date);
}

function Mail() {
  const app = useApp();
  const messages = () => app.runtime.mail();
  const unread = () => messages().filter((message) => message.unread).length;
  const [reading, setReading] = createSignal(false);
  const [error, setError] = createSignal<string>();
  const [result, setResult] = createSignal<string>();

  const read = async () => {
    if (reading()) return;
    setReading(true);
    setError(undefined);
    setResult(undefined);
    try {
      const scan = await app.runtime.readAppleMail();
      setResult(`Read ${scan.fetched} header${scan.fetched === 1 ? "" : "s"}; ${scan.appended} added to the local snapshot.`);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : typeof cause === "string" ? cause : "Apple Mail could not be read.");
    } finally {
      setReading(false);
    }
  };

  return <section class="mail-surface" aria-label="Mail">
    <header class="mail-header">
      <div>
        <h2 class="mail-title">Mail</h2>
        <p class="mail-subtitle">
          {messages().length} message{messages().length === 1 ? "" : "s"}
          {unread() > 0 ? ` · ${unread()} unread` : ""} · metadata only, read from this machine
        </p>
      </div>
      <button class="mail-read" disabled={reading()} onClick={() => void read()}>
        {reading() ? "Reading…" : "Read Apple Mail"}
      </button>
    </header>
    <p class="mail-policy">Reads up to 50 inbox headers into the local snapshot: sender, subject, date, and unread state. Message bodies stay unread.</p>
    <Show when={error() ?? app.runtime.mailError()}><p class="mail-error" role="alert">{error() ?? app.runtime.mailError()}</p></Show>
    <Show when={result()}><p class="mail-result" role="status">{result()}</p></Show>

    <Show when={messages().length > 0} fallback={<div class="mail-empty">
      <Show when={app.runtime.mailLoaded()} fallback={<p>Reading the local snapshot…</p>}>
        <p>No messages in the local snapshot yet.</p>
        <p>Choose Read Apple Mail to refresh inbox metadata from Mail.app.</p>
      </Show>
    </div>}>
      <ul class="mail-list">
        <For each={messages()}>{(message) => <li class="mail-row" classList={{ "mail-row-unread": message.unread }}>
          <span class="mail-sender">{message.sender}<Show when={message.account}><small class="mail-account">{message.account}</small></Show></span>
          <span class="mail-subject">{message.subject}</span>
          <time class="mail-date" dateTime={message.date}>{displayDate(message.date)}</time>
          <span class="mail-read-state">{message.unread ? "Unread" : "Read"}</span>
        </li>}</For>
      </ul>
    </Show>
  </section>;
}

export const mailSurface: SurfaceModule = {
  id: "mail",
  label: "Mail",
  glyph: "✉",
  caption: "mail window",
  Component: Mail,
  preview: (app) => {
    const count = app.runtime.mail().length;
    const unread = app.runtime.mail().filter((message) => message.unread).length;
    return {
      title: "Mail",
      lines: [
        count === 0 ? "no local snapshot yet" : `${count} messages · ${unread} unread`,
        "metadata read locally",
      ],
    };
  },
  refresh: (app) => app.runtime.loadMail(),
};
