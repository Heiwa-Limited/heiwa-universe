import "./styles.css";
import { apiPost, runtimeHealth, runtimeStatus, runtimeVersion, type RuntimeHealth } from "./runtime";

type Message = {
  role: "user" | "assistant" | "system";
  body: string;
  meta?: string;
};

const appElement = document.querySelector<HTMLDivElement>("#app");
if (!appElement) throw new Error("#app missing");
const app = appElement;

let health: RuntimeHealth | null = null;
let busy = false;
const messages: Message[] = [
  {
    role: "system",
    body: "Heiwa Desktop is a native Tauri 2 shell over the local runtime. One output layer; Intake, Execution, Evidence underneath.",
    meta: "local-first",
  },
];

const nav = [
  ["New session", "⌘N"],
  ["Skills & Tools", ""],
  ["Messaging", ""],
  ["Artifacts", ""],
  ["Today", "life"],
  ["Providers", "route"],
  ["Approvals", "gate"],
  ["Receipts", "proof"],
  ["Crons", "time"],
];

function escapeHtml(value: string): string {
  return value.replace(/[&<>"]/g, (char) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
  })[char] ?? char);
}

function renderMessages(): string {
  return messages.map((message) => `
    <article class="message ${message.role}">
      <div class="message-meta"><span>${message.role}</span><span>${message.meta ?? new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</span></div>
      <p>${escapeHtml(message.body)}</p>
    </article>
  `).join("");
}

function render(): void {
  const status = runtimeStatus(health);
  const reachable = health?.reachable ?? false;
  app.innerHTML = `
    <div class="desktop-shell">
      <aside class="sidebar">
        <div class="window-dots"><i></i><i></i><i></i></div>
        <div class="brand"><strong>Heiwa</strong><span>.app</span></div>
        <div class="project-chip"><b></b> ~/heiwa-universe</div>
        <nav class="primary-nav">
          ${nav.map(([label, hint], index) => `<button class="${index === 0 ? "primary" : ""}"><span>${label}</span><em>${hint}</em></button>`).join("")}
        </nav>
        <section class="rail-section"><h3>Pinned</h3><p>Shift-click a thread to pin</p></section>
        <section class="rail-section sessions"><h3>Sessions 1</h3><button class="active-dot">Heiwa Desktop Package</button></section>
      </aside>

      <main class="workspace">
        <header class="topbar">
          <div><strong>Heiwa Desktop</strong><span> native Tauri 2 package</span></div>
          <div class="top-actions"><button>Voice</button><button>Settings</button><button>Inspector</button></div>
        </header>

        <section class="conversation">
          <div class="hero-line">
            <div><p>ONE OUTPUT LAYER</p><h1>Think bigger than Hermes.</h1></div>
            <div class="plane-pills"><span>Intake</span><span>Execution</span><span>Evidence</span></div>
          </div>
          ${renderMessages()}
        </section>

        <form class="composer" id="composer">
          <button type="button" aria-label="attach">+</button>
          <textarea id="prompt" rows="2" placeholder="Ask Heiwa to inspect, plan, execute, brief, or optimize your day…"></textarea>
          <button type="submit" ${busy ? "disabled" : ""}>↑</button>
        </form>
      </main>

      <aside class="inspector">
        <h2>Runtime</h2>
        <span class="status ${reachable ? "online" : "offline"}">${status}</span>
        <dl>
          <div><dt>Version</dt><dd>${runtimeVersion(health)}</dd></div>
          <div><dt>App port</dt><dd>${reachable ? "7474" : "offline"}</dd></div>
          <div><dt>Package</dt><dd>Tauri 2 · DMG/App</dd></div>
        </dl>
        <h2>Heiwa differences</h2>
        <ul>
          <li>Life brief, calendar, automations, crons</li>
          <li>Per-device model/resource scoring</li>
          <li>Local model lanes for cheap/token-heavy work</li>
          <li>Receipts and citations for completed tasks</li>
        </ul>
      </aside>

      <footer class="statusbar">
        <span>⌘ Gateway ${reachable ? "ready" : "offline"}</span>
        <span>✦ Agents</span>
        <span>◷ Cron</span>
        <span>Context local-first</span>
        <span>⚡ Heiwa v0.1.0 · Tauri 2</span>
      </footer>
    </div>
  `;

  const form = document.querySelector<HTMLFormElement>("#composer");
  const prompt = document.querySelector<HTMLTextAreaElement>("#prompt");
  form?.addEventListener("submit", async (event) => {
    event.preventDefault();
    const text = prompt?.value.trim() ?? "";
    if (!text || busy) return;
    messages.push({ role: "user", body: text });
    if (prompt) prompt.value = "";
    busy = true;
    render();
    try {
      const response = await apiPost<{ ok?: boolean; data?: { response?: string } }>("/api/v1/repl", { prompt: text });
      messages.push({ role: "assistant", body: response.data?.response ?? "Done.", meta: "runtime" });
    } catch (error) {
      messages.push({ role: "system", body: `Runtime blocked: ${error instanceof Error ? error.message : JSON.stringify(error)}`, meta: "offline" });
    } finally {
      busy = false;
      render();
    }
  });
}

async function boot(): Promise<void> {
  render();
  health = await runtimeHealth().catch((error) => ({ reachable: false, error, snapshot: null }));
  render();
}

void boot();
