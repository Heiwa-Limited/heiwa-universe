import { For, Show } from "solid-js";
import { useApp } from "../../state/app";
import type { SurfaceModule } from "../types";
import "./mail.css";

/**
 * Mail, from the machine rather than the cloud.
 *
 * `heiwa mail scan` reads the user's own Mail.app — sender, subject, date,
 * read state, never a body — and writes a snapshot under the config root.
 * The runtime serves it at `/api/v1/mail/summary`. No OAuth, no IMAP
 * credentials, and nothing leaves the machine.
 *
 * This surface previously rendered a "reads land on the L3 connector plane"
 * placeholder while that pipeline already worked, which made a shipped
 * capability look unbuilt.
 */
function Mail() {
  const app = useApp();
  const messages = () => app.runtime.mail();
  const unread = () => messages().filter((message) => message.unread).length;

  return (
    <section class="mail-surface" aria-label="Mail">
      <header class="mail-header">
        <h2 class="mail-title">Mail</h2>
        <p class="mail-subtitle">
          {messages().length} message{messages().length === 1 ? "" : "s"}
          {unread() > 0 ? ` · ${unread()} unread` : ""} · metadata only, read
          from this machine
        </p>
      </header>

      <Show
        when={messages().length > 0}
        fallback={
          <div class="mail-empty">
            <Show
              when={app.runtime.mailLoaded()}
              fallback={<p>Reading the local snapshot…</p>}
            >
              {/*
                An empty snapshot is a state with an action, not an absent
                feature. Naming the command is the whole difference.
              */}
              <p>No messages in the local snapshot yet.</p>
              <p class="mail-empty-action">
                Run <code>heiwa mail scan</code> to read your Mail.app inbox.
                Subjects and senders only — bodies are never read.
              </p>
            </Show>
          </div>
        }
      >
        <ul class="mail-list">
          <For each={messages()}>
            {(message) => (
              <li class="mail-row" classList={{ "mail-row-unread": message.unread }}>
                <span class="mail-sender">{message.sender}</span>
                <span class="mail-subject">{message.subject}</span>
                <Show when={message.account}>
                  <span class="mail-account">{message.account}</span>
                </Show>
              </li>
            )}
          </For>
        </ul>
      </Show>

      <footer class="mail-footer">
        Sending stays approval-gated and records a receipt.
      </footer>
    </section>
  );
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
        "read locally · send gated",
      ],
    };
  },
  refresh: (app) => app.runtime.loadMail(),
};
