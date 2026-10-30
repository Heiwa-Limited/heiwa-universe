import { createEffect, createSignal, For, Index, Show } from "solid-js";
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

  const [actionError, setActionError] = createSignal<string>();
  const [stopping, setStopping] = createSignal<string>();
  const [deciding, setDeciding] = createSignal<string>();
  const actions = () => Object.keys(snapshot().toolCalls).slice(props.compact ? -8 : -40);
  const failedTurns = () => snapshot().turns.filter((turn) => ["interrupted", "blocked"].includes(turn.status)).slice(props.compact ? -3 : -20);
  const failureFor = (turnId: string) => Object.values(snapshot().routesByCall).filter((route) => route.turnId === turnId && route.eventType === "route_failed").at(-1)?.payload.message;
  const approvals = () => Object.values(snapshot().approvals).filter((row) => row.payload.outcome === "pending" && typeof row.payload.request_id === "string");
  const activeTurns = () => snapshot().turns.filter((turn) => ["open", "running", "cancelling"].includes(turn.status));
  const decide = async (id: string, approved: boolean) => {
    if (deciding()) return;
    setDeciding(id); setActionError(undefined);
    try { await app.runtime.decideApproval(id, approved); }
    catch { setActionError("The decision could not be confirmed. Check approvals before retrying."); }
    finally { setDeciding(undefined); }
  };
  const stop = async (id: string) => {
    if (stopping() === id) return;
    setStopping(id); setActionError(undefined);
    try { await app.operator.cancel(id); }
    catch { setStopping(undefined); setActionError("The stop request could not be confirmed. Reconnect to check its saved state."); }
  };

  let scroller: HTMLDivElement | undefined;
  // Follow the tail as durable rows land and as tokens stream in.
  createEffect(() => {
    snapshot();
    if (scroller) scroller.scrollTop = scroller.scrollHeight;
  });

  return (
    <div class={props.compact ? "conversation-preview" : "view ai-view"}>
      <div class="chat-messages" ref={scroller} id={props.compact ? undefined : "chat-messages"}>
        <Show when={app.operator.status() === "error"}>
          <div class="operator-compat-warning" role="status" aria-label="Connection status">
            <p>
              {app.operator.error() === "operator_submission_unavailable"
                ? "Message delivery could not be confirmed. Reconnect to check the saved conversation before sending again."
                : "Live updates are unavailable. Reconnect to load the latest saved conversation."}
            </p>
            <button class="btn-secondary" onClick={() => void app.operator.reconnect()}>
              Reconnect
            </button>
          </div>
        </Show>
        <Show when={app.operator.status() === "starting"}>
          <div role="status">Loading saved conversation…</div>
        </Show>
        <Show when={snapshot().compatibility.unsupportedSchemaEvents > 0}>
          <div class="operator-compat-warning" role="status">
            Skipped {snapshot().compatibility.unsupportedSchemaEvents} operator{" "}
            {snapshot().compatibility.unsupportedSchemaEvents === 1 ? "event" : "events"} from a
            newer schema. Cursor progress is preserved; update Heiwa Desktop to interpret them.
          </div>
        </Show>

        <Show when={isEmpty() && app.operator.status() !== "error" && app.operator.status() !== "starting"}>
          <div class="chat-empty">
            <div class="chat-empty-icon"><Icon name="heiwa" size={36} /></div>
            <p>No messages yet.</p>
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

        <Show when={actions().length || activeTurns().length || approvals().length || failedTurns().length}>
          <section class="execution-timeline" aria-label="Actions and progress">
            <For each={activeTurns()}>{(turn) => <div class="execution-status" role="status">
              <span>{turn.cancelRequested || stopping() === turn.turnId ? "Stopping…" : "Working…"}</span>
              <button type="button" class="btn-secondary" disabled={turn.cancelRequested || stopping() === turn.turnId} onClick={() => void stop(turn.turnId)}>Stop</button>
            </div>}</For>
            <For each={actions()}>{(key) => {
              const action = () => snapshot().toolCalls[key];
              const details = () => action().payload.arguments ?? action().payload.output_preview ?? action().payload.error ?? "";
              const verified = () => action().payload.verified_result as Record<string, unknown> | undefined;
              return <div>
                <Show when={verified()?.kind}><div class="verified-action" aria-label="Runtime verified result">
                  <strong>{verifiedLabel(String(verified()!.kind))}</strong>
                  <Show when={verified()?.path}><code>{String(verified()!.path)}</code></Show>
                  <Show when={verified()?.title}><span>{String(verified()!.title)}</span></Show>
                  <Show when={verified()?.sha256}><small>SHA256: {String(verified()!.sha256)}</small></Show>
                  <Show when={verified()?.kind === "calendar_proposal"}><small>Approval reference: {String(verified()!.request_id ?? "local hold")}</small></Show>
                </div></Show>
                <details class="execution-action">
                <summary>{actionLabel(String(action().payload.name ?? "Action"))} <span>{action().eventType === "tool_call_started" ? "Running" : String(action().payload.status ?? "Finished")}</span></summary>
                <pre>{(typeof details() === "string" ? String(details()) : JSON.stringify(details(), null, 2)).slice(0, 8000)}</pre>
                <Show when={typeof action().payload.receipt_id === "string"}><small>Receipt: {String(action().payload.receipt_id)}</small></Show>
              </details></div>;
            }}</For>
            <For each={failedTurns()}>{(turn) => <p class="execution-failure" role="status">{turn.cancelRequested ? "Stopped by request." : `Could not finish: ${String(failureFor(turn.turnId) ?? "Check the saved actions and approvals before continuing.")}`}</p>}</For>
            <For each={approvals()}>{(approval) => <div class="execution-approval" role="group" aria-label="Action approval">
              <strong>Approve {String(approval.payload.tool ?? "action")}</strong>
              <p>Review the action’s arguments above before continuing.</p>
              <button type="button" disabled={!!deciding()} onClick={() => void decide(String(approval.payload.request_id), true)}>Approve action</button>
              <button type="button" disabled={!!deciding()} onClick={() => void decide(String(approval.payload.request_id), false)}>Deny action</button>
            </div>}</For>
          </section>
        </Show>
        <Show when={actionError()}><p role="alert">{actionError()}</p></Show>
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

function actionLabel(name: string): string {
  const labels: Record<string, string> = {
    "fs.list": "List files", "fs.read": "Read file", "repo.grep": "Search files", "artifact.write": "Save artifact",
    "calendar.list": "List calendars", "calendar.events": "Read Calendar", "calendar.stage": "Prepare Calendar event",
    "reminders.lists": "List reminder lists", "reminders.read": "Read Reminders", "reminders.create": "Create reminder", "reminders.complete": "Update reminder completion",
    "mail.summary": "Read Mail snapshot", "notes.search": "Search Notes", "notes.read": "Read note", "notes.folders": "List Notes folders", "notes.create": "Create note",
    "finder.reveal": "Reveal in Finder", "terminal.open": "Open Terminal", "process.run": "Run workspace command", "workflows.run": "Run workflow",
  };
  return labels[name] ?? name;
}

function verifiedLabel(kind: string): string {
  return ({ file: "File saved and verified", note: "Apple Note saved and verified", reminder: "Reminder saved and verified", process: "Workspace command completed", calendar_proposal: "Calendar proposal staged" } as Record<string, string>)[kind] ?? "Verified action";
}
