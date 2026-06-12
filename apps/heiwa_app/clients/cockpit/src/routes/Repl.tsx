import { For, Show, createSignal } from "solid-js";
import type { JSX } from "solid-js";
import { postSse } from "../lib/api";
import type { ReplRouteEvent, ReplTrace } from "../lib/types";

type Message = {
  id: string;
  role: "user" | "assistant" | "system";
  content: string;
  route?: ReplRouteEvent;
  trace?: ReplTrace;
  streaming?: boolean;
  ts: string;
};

const starterMessages: Message[] = [
  {
    id: "system-ready",
    role: "system",
    content:
      "Heiwa is connected to the local runtime. Ask once; Heiwa routes through Intake, Execution, and Evidence.",
    ts: "now",
  },
];

function nowLabel(): string {
  return new Date().toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
  });
}

function codeLike(content: string): boolean {
  const trimmed = content.trim();
  return (
    trimmed.startsWith("{") ||
    trimmed.startsWith("[") ||
    trimmed.includes("```json")
  );
}

function TracePills(props: {
  route?: ReplRouteEvent | undefined;
  trace?: ReplTrace | undefined;
}): JSX.Element {
  const provider = () => props.trace?.provider ?? props.route?.provider;
  const model = () => props.trace?.model ?? props.route?.model;
  const mode = () => props.trace?.mode ?? props.route?.mode;
  const privacy = () => props.trace?.privacy ?? props.route?.privacy;
  return (
    <Show when={mode()}>
      <div class="trace-pills" aria-label="Execution trace">
        <span>mode: {mode()}</span>
        <Show when={privacy() === "sovereign"}>
          <span>sovereign · local-only</span>
        </Show>
        <Show when={provider()}>
          <span>
            route: {provider()}/{model()}
          </span>
        </Show>
        <Show when={props.trace?.cost_usd !== undefined}>
          <span>cost: ${props.trace?.cost_usd?.toFixed(4)}</span>
        </Show>
        <Show when={props.trace?.compression?.applied}>
          <span>compressed ×{props.trace?.compression?.ratio.toFixed(2)}</span>
        </Show>
      </div>
    </Show>
  );
}

function MessageCard(props: { message: Message }): JSX.Element {
  return (
    <article
      class={`repl-message ${props.message.role}`}
      classList={{ thinking: props.message.streaming === true }}
    >
      <div class="repl-message-meta">
        <span>{props.message.role === "assistant" ? "Heiwa" : props.message.role}</span>
        <span>
          {props.message.streaming
            ? (props.message.route
                ? `${props.message.route.provider ?? "routing"}…`
                : "routing…")
            : props.message.ts}
        </span>
      </div>
      <Show
        when={codeLike(props.message.content)}
        fallback={<p>{props.message.content || "…"}</p>}
      >
        <div class="code-card repl-code-card">
          <div class="code-card-header">
            <span>{"{ }"}</span>
            <strong>Code</strong>
            <em>json/text</em>
          </div>
          <pre>{props.message.content}</pre>
        </div>
      </Show>
      <TracePills route={props.message.route} trace={props.message.trace} />
    </article>
  );
}

export default function ReplRoute(): JSX.Element {
  const [messages, setMessages] = createSignal<Message[]>(starterMessages);
  const [prompt, setPrompt] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  function patchMessage(id: string, patch: Partial<Message>): void {
    setMessages((items) =>
      items.map((item) => (item.id === id ? { ...item, ...patch } : item)),
    );
  }

  function appendToMessage(id: string, text: string): void {
    setMessages((items) =>
      items.map((item) =>
        item.id === id ? { ...item, content: item.content + text } : item,
      ),
    );
  }

  async function submit(): Promise<void> {
    const text = prompt().trim();
    if (!text || busy()) return;

    const userMessage: Message = {
      id: `user-${Date.now()}`,
      role: "user",
      content: text,
      ts: nowLabel(),
    };
    const assistantId = `assistant-${Date.now()}`;
    setMessages((items) => [
      ...items,
      userMessage,
      {
        id: assistantId,
        role: "assistant",
        content: "",
        streaming: true,
        ts: nowLabel(),
      },
    ]);
    setPrompt("");
    setBusy(true);
    setError(null);

    try {
      await postSse("/api/v1/repl/stream", { prompt: text }, (event) => {
        if (event.event === "route") {
          patchMessage(assistantId, { route: event.data as ReplRouteEvent });
        } else if (event.event === "token") {
          const token = (event.data as { text?: string }).text ?? "";
          appendToMessage(assistantId, token);
        } else if (event.event === "done") {
          patchMessage(assistantId, {
            trace: event.data as ReplTrace,
            streaming: false,
            ts: nowLabel(),
          });
        } else if (event.event === "error") {
          const message =
            (event.data as { message?: string }).message ?? "stream failed";
          throw new Error(message);
        }
      });
      // Guard: if the stream ended without a done event, settle the bubble.
      patchMessage(assistantId, { streaming: false });
    } catch (err) {
      const message =
        err instanceof Error
          ? err.message
          : ((err as { message?: string }).message ?? String(err));
      setError(message);
      setMessages((items) =>
        items
          .filter((item) => !(item.id === assistantId && !item.content))
          .map((item) =>
            item.id === assistantId ? { ...item, streaming: false } : item,
          ),
      );
      setMessages((items) => [
        ...items,
        {
          id: `error-${Date.now()}`,
          role: "system",
          content: `Blocked: ${message}`,
          ts: nowLabel(),
        },
      ]);
    } finally {
      setBusy(false);
    }
  }

  function onKeyDown(event: KeyboardEvent): void {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      void submit();
    }
  }

  return (
    <section class="repl-shell" aria-label="Heiwa conversation">
      <div class="conversation-stream">
        <div class="conversation-title-row">
          <div>
            <p class="eyebrow">One output layer</p>
            <h1>Heiwa Conversation</h1>
          </div>
          <div class="runtime-chip-row">
            <span>Intake</span>
            <span>Execution</span>
            <span>Evidence</span>
          </div>
        </div>

        <For each={messages()}>{(message) => <MessageCard message={message} />}</For>
      </div>

      <Show when={error()}>
        {(value) => <div class="repl-error">{value()}</div>}
      </Show>

      <form
        class="composer-bar"
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
      >
        <button class="composer-plus" type="button" aria-label="Attach context">
          +
        </button>
        <textarea
          class="composer-input"
          rows={2}
          value={prompt()}
          placeholder="Ask Heiwa to inspect, plan, execute, or brief you…"
          onInput={(event) => setPrompt(event.currentTarget.value)}
          onKeyDown={onKeyDown}
        />
        <button
          class="composer-submit"
          type="submit"
          disabled={busy() || !prompt().trim()}
        >
          ↑
        </button>
      </form>
    </section>
  );
}
