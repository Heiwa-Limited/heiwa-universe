import { createEffect, createSignal, onCleanup, onMount, Show } from "solid-js";
import { providersFromSnapshot, runtimeVersion } from "../runtime";
import { useApp } from "../state/app";
import { Conversation } from "./Conversation";
import { Icon } from "./Icon";

/**
 * The always-present input. Responses open in place, preserving the active
 * surface; expansion presents the same durable conversation.
 */
export function Composer(props: { caption: string }) {
  const app = useApp();
  let input: HTMLTextAreaElement | undefined;
  const [responseOpen, setResponseOpen] = createSignal(false);
  const [sending, setSending] = createSignal(false);
  const [error, setError] = createSignal<string>();
  onMount(() => {
    const focusChatbar = (event: KeyboardEvent) => {
      if (!event.metaKey || event.altKey || event.ctrlKey || event.key.toLowerCase() !== "l") return;
      if (input?.closest("[inert]") || document.activeElement?.closest('[role="dialog"]')) return;
      event.preventDefault();
      input?.focus();
    };
    document.addEventListener("keydown", focusChatbar);
    onCleanup(() => document.removeEventListener("keydown", focusChatbar));
  });
  createEffect(() => {
    const draft = app.sessions.draft();
    if (input && input.value !== draft) {
      input.value = draft;
      input.style.height = "auto";
    }
  });

  const connectedProviders = () =>
    providersFromSnapshot(app.runtime.health()).filter(
      (provider) => provider.status === "connected",
    ).length;
  const conversationTitle = () => app.sessions.threads()
    .find((thread) => thread.thread_id === app.sessions.selectedId())?.title || "New conversation";

  const send = async () => {
    // DOM is the source during an in-flight keystroke; the session map is
    // updated on every input and restores it when the selected ID changes.
    const sessionId = app.sessions.selectedId();
    const draft = input?.value ?? app.sessions.draft();
    const text = draft.trim();
    if (!text || !app.operator.ready() || sending()) return;
    setSending(true);
    setError(undefined);
    setResponseOpen(true);
    try {
      await app.operator.submit(text);
      // Do not erase text the user typed while the request was in flight.
      if (sessionId) app.sessions.clearDraftIfUnchanged(sessionId, draft);
      void app.sessions.refresh();
      if (input && app.sessions.selectedId() === sessionId && input.value === draft) {
        input.value = "";
        input.style.height = "auto";
      }
    } catch {
      setError("Message not acknowledged. Your draft is retained; check the conversation before retrying.");
    } finally {
      setSending(false);
    }
  };

  return (
    <div class="composer-area">
      <Show when={responseOpen() && app.view() !== "ai"}>
        <section class="response-panel" role="region" aria-label="Heiwa response" onKeyDown={(event) => {
          if (event.key === "Escape") { setResponseOpen(false); input?.focus(); }
        }}>
          <header class="response-panel-header">
            <strong><Icon name="heiwa" size={18} />Heiwa</strong>
            <div>
              <button onClick={() => { app.navigate("ai"); setResponseOpen(false); }}>Open conversation</button>
              <button aria-label="Close response" onClick={() => { setResponseOpen(false); input?.focus(); }}><Icon name="close" size={16} /></button>
            </div>
          </header>
          <Conversation compact />
        </section>
      </Show>
      <Show when={error()}><p class="composer-error" role="alert">{error()}</p></Show>
      <div class="composer-context" aria-label={props.caption}>
        <span>{app.view() === "ai" ? conversationTitle() : `Viewing ${app.view() === "projects" ? "project" : app.view() === "sessions" ? "all sessions" : app.view()}`}</span>
        <Show when={app.view() !== "ai" && !responseOpen()}>
          <button class="response-reopen" onClick={() => setResponseOpen(true)}><Icon name="sessions" size={14} />Show conversation</button>
        </Show>
      </div>
      <div class="composer-wrap">
        <span class="composer-mark"><Icon name="heiwa" size={22} /></span>
        <textarea
          ref={input}
          rows="1"
          placeholder="Message Heiwa…"
          aria-label="Message Heiwa"
          onKeyDown={(event) => {
            if (event.key === "Escape") setResponseOpen(false);
            if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
              event.preventDefault();
              void send();
            }
          }}
          onInput={(event) => {
            const el = event.currentTarget;
            app.sessions.setDraft(el.value);
            el.style.height = "auto";
            el.style.height = `${Math.min(el.scrollHeight, 120)}px`;
          }}
        />
        <button
          class="composer-send"
          disabled={!app.operator.ready() || sending() || !app.sessions.draft().trim()}
          aria-label="Send"
          onClick={() => void send()}
        >
          <Icon name="send" size={19} />
        </button>
      </div>
      <div class="composer-hint">
        <span class="hint">Enter to send · Shift Enter for a new line · ⌘L to focus</span>
        <span class="hint">
          {runtimeVersion(app.runtime.health())} · {connectedProviders()} providers
        </span>
      </div>
    </div>
  );
}
