import { For, Show, createSignal } from "solid-js";
import type { JSX } from "solid-js";
import { api } from "../lib/api";

type ReplTrace = {
  route?: string;
  provider?: string;
  model?: string;
  mode?: string;
  receipts?: string[];
  [key: string]: unknown;
};

type ReplResponse = {
  ok: boolean;
  data?: {
    response?: string;
    trace?: ReplTrace | undefined;
  };
  error?: {
    code?: string;
    message?: string;
  };
};

type Message = {
  id: string;
  role: "user" | "assistant" | "system";
  content: string;
  trace?: ReplTrace | undefined;
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
  return new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

function codeLike(content: string): boolean {
  const trimmed = content.trim();
  return trimmed.startsWith("{") || trimmed.startsWith("[") || trimmed.includes("```json");
}

function TracePills(props: { trace?: ReplTrace | undefined }): JSX.Element {
  const trace = () => props.trace;
  return (
    <Show when={trace()}>
      {(value) => (
        <div class="trace-pills" aria-label="Execution trace">
          <span>route: {String(value().route ?? value().mode ?? "local")}</span>
          <span>provider: {String(value().provider ?? "heiwa")}</span>
          <span>model: {String(value().model ?? "deterministic")}</span>
        </div>
      )}
    </Show>
  );
}

function MessageCard(props: { message: Message }): JSX.Element {
  return (
    <article class={`repl-message ${props.message.role}`}>
      <div class="repl-message-meta">
        <span>{props.message.role === "assistant" ? "Heiwa" : props.message.role}</span>
        <span>{props.message.ts}</span>
      </div>
      <Show
        when={codeLike(props.message.content)}
        fallback={<p>{props.message.content}</p>}
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
      <TracePills trace={props.message.trace} />
    </article>
  );
}

export default function ReplRoute(): JSX.Element {
  const [messages, setMessages] = createSignal<Message[]>(starterMessages);
  const [prompt, setPrompt] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  async function submit(): Promise<void> {
    const text = prompt().trim();
    if (!text || busy()) return;

    const userMessage: Message = {
      id: `user-${Date.now()}`,
      role: "user",
      content: text,
      ts: nowLabel(),
    };
    setMessages((items) => [...items, userMessage]);
    setPrompt("");
    setBusy(true);
    setError(null);

    try {
      const result = await api.post<ReplResponse>("/api/v1/repl", { prompt: text });
      if (!result.ok) {
        throw new Error(result.error?.message || result.error?.code || "REPL request failed");
      }
      setMessages((items) => [
        ...items,
        {
          id: `assistant-${Date.now()}`,
          role: "assistant",
          content: result.data?.response || "Done.",
          trace: result.data?.trace,
          ts: nowLabel(),
        },
      ]);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
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

        <Show when={busy()}>
          <article class="repl-message assistant thinking">
            <div class="repl-message-meta">
              <span>Heiwa</span>
              <span>routing…</span>
            </div>
            <p>Checking local policy, route, and evidence lane.</p>
          </article>
        </Show>
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
        <button class="composer-submit" type="submit" disabled={busy() || !prompt().trim()}>
          ↑
        </button>
      </form>
    </section>
  );
}
