import type { JSX } from "solid-js";
import { For, Show, createMemo, createSignal, onCleanup } from "solid-js";

type InputMode = "text" | "voice" | "image" | "video";

interface Artifact {
  title: string;
  file: string;
  badge: "review" | "verified";
  summary: string;
}

interface Message {
  id: string;
  role: "user" | "assistant";
  text: string;
  trace?: string;
  media?: string;
  artifacts?: Artifact[];
  diff?: {
    files: number;
    additions: number;
    deletions: number;
  };
}

const contextSignals = [
  {
    label: "Browser",
    state: "attached",
    detail: "Visible page, console health, screenshots",
  },
  {
    label: "Email",
    state: "priority",
    detail: "Metadata summaries and pinned asks",
  },
  {
    label: "Messages",
    state: "planned",
    detail: "Discord, Telegram, iMessage, SMS",
  },
  {
    label: "Machine",
    state: "live",
    detail: "CPU, RAM, local model availability",
  },
  {
    label: "Actions",
    state: "gated",
    detail: "Computer use and writes require approval",
  },
  {
    label: "Projects",
    state: "managed",
    detail: "Durable work grouped without manual ticketing",
  },
];

const initialArtifacts: Artifact[] = [
  {
    title: "Local Release Sandbox",
    file: "scripts/package_release_sandbox.sh",
    badge: "verified",
    summary:
      "Builds the host release binary, packages the archive, writes checksums, and smokes the packaged heiwa binary without upload.",
  },
  {
    title: "Runtime Authority Contract",
    file: "docs/deployment.md",
    badge: "review",
    summary:
      "Separates installed GitHub-release runtime from checkout development mode and keeps Cloudflare/STDB as protected backends.",
  },
  {
    title: "Surface Context Packet",
    file: "~/.heiwa/state/inbox",
    badge: "review",
    summary:
      "Normalizes browser, mail, messages, machine telemetry, and integration alerts into one chat-readable context stream.",
  },
  {
    title: "Human/Machine Ops Bridge",
    file: "HEIWA.md#interaction-contract",
    badge: "verified",
    summary:
      "Keeps the user in one executive-assistant conversation while Heiwa coordinates AI providers, local machines, browser state, and approval-gated actions.",
  },
  {
    title: "HOME App Launcher",
    file: "~/.heiwa/app/Heiwa.app",
    badge: "review",
    summary:
      "Makes the primary display an installed executable app path while the browser console stays user-scoped support infrastructure.",
  },
];

const starterMessages: Message[] = [
  {
    id: "assistant-boot",
    role: "assistant",
    text:
      "Heiwa.app is the primary installed input/display surface. Talk to Heiwa here; the runtime watches connected surfaces in the background, explains what changed, stages risky actions, auto-groups projects, and returns receipts in this same stream.",
    trace:
      "io=single-thread; surfaces=browser,mail,machine,computer-use,integrations; writes=approval-gated",
    artifacts: initialArtifacts,
    diff: { files: 4, additions: 198, deletions: 0 },
  },
];

export default function Dashboard(): JSX.Element {
  const [inputMode, setInputMode] = createSignal<InputMode>("text");
  const [inputText, setInputText] = createSignal("");
  const [isRecording, setIsRecording] = createSignal(false);
  const [isAnalyzing, setIsAnalyzing] = createSignal(false);
  const [uploadedMedia, setUploadedMedia] = createSignal<string | null>(null);
  const [voiceWave, setVoiceWave] = createSignal<number[]>([]);
  const [messages, setMessages] = createSignal<Message[]>(starterMessages);
  let waveInterval: ReturnType<typeof setInterval> | undefined;

  const modeLabel = createMemo(() => {
    switch (inputMode()) {
      case "voice":
        return "Voice";
      case "image":
        return "Image";
      case "video":
        return "Video";
      default:
        return "Text";
    }
  });

  onCleanup(() => {
    if (waveInterval) clearInterval(waveInterval);
  });

  const setMode = (mode: InputMode) => {
    setInputMode(mode);
    if (mode !== "voice" && waveInterval) {
      clearInterval(waveInterval);
      setIsRecording(false);
      setVoiceWave([]);
    }
  };

  const toggleVoiceRecording = () => {
    if (isRecording()) {
      setIsRecording(false);
      if (waveInterval) clearInterval(waveInterval);
      setVoiceWave([]);
      setInputText("Voice instruction captured for local review.");
      return;
    }

    setInputMode("voice");
    setIsRecording(true);
    waveInterval = setInterval(() => {
      setVoiceWave(Array.from({ length: 22 }, () => Math.floor(Math.random() * 26) + 6));
    }, 120);
  };

  const stageMedia = (type: "image" | "video") => {
    setInputMode(type);
    setIsAnalyzing(true);
    setTimeout(() => {
      setIsAnalyzing(false);
      setUploadedMedia(
        type === "image" ? "runtime-screenshot.png" : "browser-verification.mov",
      );
    }, 500);
  };

  const clearMedia = () => {
    setUploadedMedia(null);
  };

  const submitPrompt = (e: Event) => {
    e.preventDefault();
    const prompt = inputText().trim();
    const media = uploadedMedia();
    if (!prompt && !media) return;

    const userText = prompt || `Review attached ${inputMode()} evidence.`;
    const userMessage: Message = {
      id: `user-${Date.now()}`,
      role: "user",
      text: userText,
    };
    if (media) userMessage.media = media;
    setMessages((prev) => [...prev, userMessage]);
    setInputText("");
    clearMedia();

    fetch("/api/v1/repl", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ prompt: userText }),
    })
      .then((res) => {
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        return res.json();
      })
      .then((data) => {
        setMessages((prev) => [
          ...prev,
          {
            id: `assistant-${Date.now()}`,
            role: "assistant",
            text: data.ok
              ? data.data.response
              : `Execution failed: ${data.error?.message ?? "unknown error"}`,
            trace: data.ok ? data.data.trace : "status=failed",
          },
        ]);
      })
      .catch((err) => {
        setMessages((prev) => [
          ...prev,
          {
            id: `assistant-${Date.now()}`,
            role: "assistant",
            text: `Local runtime request failed: ${err.message ?? "unknown error"}`,
            trace: "status=offline",
          },
        ]);
      });
  };

  return (
    <section class="workspace-dashboard">
      <header class="session-hero">
        <div>
          <p class="session-kicker">Primary interface</p>
          <h1>One conversation with Heiwa</h1>
          <p class="session-copy">
            Ask once, then let the runtime gather context from connected surfaces,
            execute safe work, manage projects, and explain AI/Machine Ops back
            through this thread.
          </p>
        </div>
        <div class="session-status-strip" aria-label="Runtime status">
          <span>HOME-installed app</span>
          <span>Single I/O thread</span>
          <span>Background telemetry</span>
          <span>Approval-gated writes</span>
        </div>
      </header>

      <section class="context-signal-band" aria-label="Connected surface context">
        <div class="context-band-copy">
          <span class="panel-label">Now context</span>
          <p>
            Heiwa should keep surface details in the background and interrupt only
            when intent, risk, or evidence needs the user.
          </p>
        </div>
        <div class="context-signal-grid">
          <For each={contextSignals}>
            {(signal) => (
              <div class="context-signal-card">
                <div class="context-signal-topline">
                  <span>{signal.label}</span>
                  <strong>{signal.state}</strong>
                </div>
                <p>{signal.detail}</p>
              </div>
            )}
          </For>
        </div>
      </section>

      <div class="workspace-chat-panel">
        <div class="chat-panel-header">
          <div>
            <span class="panel-label">Heiwa output</span>
            <h2>Unified context and action stream</h2>
          </div>
          <div class="panel-meta">
            <span>{messages().length} messages</span>
            <span>{modeLabel()} mode</span>
          </div>
        </div>

        <div class="workspace-message-list" aria-live="polite">
          <For each={messages()}>
            {(message) => (
              <article class={`workspace-message ${message.role}`}>
                <div class="message-avatar" aria-hidden="true">
                  {message.role === "assistant" ? "H" : "D"}
                </div>
                <div class="message-stack">
                  <p>{message.text}</p>

                  <Show when={message.media}>
                    <div class="attachment-pill">Attached: {message.media}</div>
                  </Show>

                  <Show when={message.artifacts?.length}>
                    <div class="artifact-grid">
                      <For each={message.artifacts}>
                        {(artifact) => (
                          <div class="artifact-card">
                            <div class="artifact-header-row">
                              <div class="artifact-icon" aria-hidden="true">
                                {artifact.badge === "verified" ? "✓" : "!"}
                              </div>
                              <div class="artifact-meta">
                                <span class="artifact-title">{artifact.title}</span>
                                <a class="artifact-file-ref" href="#">
                                  {artifact.file}
                                </a>
                              </div>
                              <span class={`artifact-badge ${artifact.badge}`}>
                                {artifact.badge}
                              </span>
                            </div>
                            <p class="artifact-summary">{artifact.summary}</p>
                            <div class="artifact-actions">
                              <button class="btn-artifact primary" type="button">
                                Open artifact
                              </button>
                              <button class="btn-artifact" type="button">
                                Pin evidence
                              </button>
                            </div>
                          </div>
                        )}
                      </For>
                    </div>
                  </Show>

                  <Show when={message.diff}>
                    {(diff) => (
                      <div class="diff-capsule">
                        <span class="diff-count">{diff().files} files changed</span>
                        <span class="diff-additions">+{diff().additions}</span>
                        <span class="diff-deletions">-{diff().deletions}</span>
                        <button class="btn-review-diff" type="button">
                          Review diff
                        </button>
                      </div>
                    )}
                  </Show>

                  <Show when={message.trace}>
                    <div class="trace-details">DREX trace: {message.trace}</div>
                  </Show>
                </div>
              </article>
            )}
          </For>
        </div>

        <form class="workspace-omni-console" onSubmit={submitPrompt}>
          <div class="input-modes-bar compact">
            <button
              type="button"
              class="mode-btn"
              classList={{ active: inputMode() === "text" }}
              onClick={() => setMode("text")}
            >
              Text
            </button>
            <button
              type="button"
              class="mode-btn"
              classList={{ active: inputMode() === "voice" }}
              onClick={toggleVoiceRecording}
            >
              {isRecording() ? "Stop voice" : "Voice"}
            </button>
            <button
              type="button"
              class="mode-btn"
              classList={{ active: inputMode() === "image" }}
              onClick={() => stageMedia("image")}
            >
              Image
            </button>
            <button
              type="button"
              class="mode-btn"
              classList={{ active: inputMode() === "video" }}
              onClick={() => stageMedia("video")}
            >
              Video
            </button>
          </div>

          <Show when={isRecording() && voiceWave().length > 0}>
            <div class="voice-wave-container compact">
              <For each={voiceWave()}>
                {(height) => <div class="wave-bar" style={{ height: `${height}px` }} />}
              </For>
            </div>
          </Show>

          <Show when={isAnalyzing()}>
            <div class="analysis-strip">Preparing local attachment evidence...</div>
          </Show>

          <Show when={uploadedMedia()}>
            <div class="queued-attachment">
              <span>{uploadedMedia()}</span>
              <button type="button" onClick={clearMedia}>
                Clear
              </button>
            </div>
          </Show>

          <div class="omni-input-bar">
            <button class="omni-action-btn" type="button" aria-label="Attach evidence">
              +
            </button>
            <textarea
              class="omni-textarea"
              rows="1"
              placeholder="Ask Heiwa to inspect, execute, summarize, or verify..."
              value={inputText()}
              onInput={(e) => setInputText(e.currentTarget.value)}
            />
            <div class="omni-actions-row">
              <div class="omni-model-selector" aria-label="Selected routing lane">
                local-first
              </div>
              <button class="omni-send-btn" type="submit" aria-label="Send prompt">
                →
              </button>
            </div>
          </div>
        </form>
      </div>
    </section>
  );
}
