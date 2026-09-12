import { createEffect, Index, Show } from "solid-js";
import { timeFmt } from "../lib/format";
import { useApp } from "../state/app";
import { Icon } from "./Icon";
import "../surfaces/ai/ai.css";

/** The full session and response panel read the same durable operator projection. */
export function Conversation(props: { compact?: boolean }) {
  const app = useApp();
  const snapshot = () => app.operator.snapshot();
  const transients = () => Object.entries(snapshot().transientByTurn);
  const isEmpty = () => snapshot().messages.length === 0 && transients().length === 0;

  let scroller: HTMLDivElement | undefined;
  // Follow the tail as durable rows land and as tokens stream in.
  createEffect(() => {
    snapshot();
    if (scroller) scroller.scrollTop = scroller.scrollHeight;
  });

  return (
    <div class={props.compact ? "conversation-preview" : "view ai-view"}>
      <div class="chat-messages" ref={scroller} id={props.compact ? undefined : "chat-messages"}>
        <Show when={snapshot().compatibility.unsupportedSchemaEvents > 0}>
          <div class="operator-compat-warning" role="status">
            Skipped {snapshot().compatibility.unsupportedSchemaEvents} operator{" "}
            {snapshot().compatibility.unsupportedSchemaEvents === 1 ? "event" : "events"} from a
            newer schema. Cursor progress is preserved; update Heiwa Desktop to interpret them.
          </div>
        </Show>

        <Show when={isEmpty()}>
          <div class="chat-empty">
            <div class="chat-empty-icon"><Icon name="heiwa" size={36} /></div>
            <p>
              {app.operator.status() === "error"
                ? "Operator stream unavailable."
                : "No messages yet."}
            </p>
          </div>
        </Show>

        {/*
          Index, not For: OperatorStore.snapshot() deep-clones its projection,
          so every message object is a fresh reference on each publish. For
          would key on identity and rebuild every row per streamed token;
          Index keys by position and updates only the fields that changed.
        */}
        <Index each={props.compact ? snapshot().messages.slice(-4) : snapshot().messages}>
          {(message) => (
            <article class={`chat-msg ${message().role}`}>
              <div class="chat-msg-header">
                <span class="chat-role">{message().role}</span>
                <span class="chat-meta">
                  {[message().provider, message().model].filter(Boolean).join("/") ||
                    (message().receiptRef ? "receipt linked" : "")}
                </span>
                <span class="chat-time">{timeFmt(Date.parse(message().occurredAt))}</span>
              </div>
              <div class="chat-body">{message().body}</div>
            </article>
          )}
        </Index>

        <Index each={transients()}>
          {(entry) => (
            <article class="chat-msg assistant transient" data-transient-turn={entry()[0]}>
              <div class="chat-msg-header">
                <span class="chat-role">assistant</span>
                <span class="chat-meta">streaming</span>
                <span class="chat-time" />
              </div>
              <div class="chat-body">{entry()[1]}</div>
            </article>
          )}
        </Index>
      </div>
    </div>
  );
}
