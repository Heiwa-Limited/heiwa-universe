import type { JSX } from "solid-js";
import { For } from "solid-js";

type MailLane = {
  name: string;
  status: "metadata" | "gated" | "planned";
  read: string;
  reply: string;
  guardrail: string;
};

type PriorityThread = {
  from: string;
  subject: string;
  signal: string;
  action: "report" | "draft" | "approval";
};

const lanes: MailLane[] = [
  {
    name: "Gmail",
    status: "gated",
    read: "OAuth read scope first; local scheduled scan by default because Heiwa has no hosted webhook runtime.",
    reply: "Draft replies in-app, then send through Gmail API only after approval.",
    guardrail: "Send scope is separate from read scope and every outbound message records an approval receipt.",
  },
  {
    name: "Apple Mail",
    status: "metadata",
    read: "Start with metadata and safe local summaries; deeper compose safety can later use MailKit extension points.",
    reply: "Stage local drafts and hand off to Mail.app until a product-grade MailKit bridge exists.",
    guardrail: "Do not treat MailKit as a general mailbox API; body reads require explicit connector permission.",
  },
  {
    name: "IMAP / Himalaya",
    status: "planned",
    read: "Portable fallback for user-owned IMAP accounts and non-Gmail providers.",
    reply: "Template replies can be sent through SMTP after approval.",
    guardrail: "Use account-scoped leases and never retry sends blindly after partial failures.",
  },
];

const threads: PriorityThread[] = [
  {
    from: "calendar-source@example.com",
    subject: "Reschedule request",
    signal: "Calendar conflict if accepted; draft negotiation before changing event.",
    action: "draft",
  },
  {
    from: "vendor@example.com",
    subject: "Invoice follow-up",
    signal: "Likely needs acknowledgement but no payment action without approval.",
    action: "approval",
  },
  {
    from: "newsletter@example.com",
    subject: "Weekly update",
    signal: "Low priority; include in digest only.",
    action: "report",
  },
];

function statusClass(status: MailLane["status"]): string {
  if (status === "metadata") return "ok";
  if (status === "gated") return "warn";
  return "fail";
}

function actionClass(action: PriorityThread["action"]): string {
  if (action === "approval") return "fail";
  if (action === "draft") return "warn";
  return "ok";
}

export default function MailRoute(): JSX.Element {
  return (
    <section>
      <div class="hero compact">
        <p class="eyebrow">Mail</p>
        <h1>Communications cockpit</h1>
        <p class="lede">
          Email becomes a governed intake lane: scan, report, summarize, draft,
          and approval-send from Heiwa.app without hidden outbound automation.
        </p>
      </div>

      <div class="panels">
        <article class="panel">
          <div class="status-card-head">
            <h2>Priority report shape</h2>
            <span class="status-badge warn">connectors pending</span>
          </div>
          <p class="muted">
            No mailbox bodies are read by this UI slice. These rows define the
            in-app pattern for future Gmail/Apple Mail/IMAP scans.
          </p>
          <ul>
            <For each={threads}>
              {(thread) => (
                <li>
                  <strong>{thread.subject}</strong>{" "}
                  <span class={`status-badge ${actionClass(thread.action)}`}>
                    {thread.action}
                  </span>
                  <p class="muted">
                    {thread.from} · {thread.signal}
                  </p>
                </li>
              )}
            </For>
          </ul>
        </article>

        <article class="panel">
          <div class="status-card-head">
            <h2>Reply safety</h2>
            <span class="status-badge ok">approval-first</span>
          </div>
          <ol>
            <li>Classify thread and sensitivity.</li>
            <li>Draft response locally or through the routing matrix.</li>
            <li>Show recipients, subject, body, attachments, source thread, and risk.</li>
            <li>Send only after approval; write a receipt with external IDs and body hash.</li>
          </ol>
        </article>
      </div>

      <div class="card-list">
        <For each={lanes}>
          {(lane) => (
            <article>
              <div class="status-card-head">
                <h3>{lane.name}</h3>
                <span class={`status-badge ${statusClass(lane.status)}`}>
                  {lane.status}
                </span>
              </div>
              <p><strong>Read:</strong> {lane.read}</p>
              <p><strong>Reply:</strong> {lane.reply}</p>
              <p class="muted"><strong>Guardrail:</strong> {lane.guardrail}</p>
            </article>
          )}
        </For>
      </div>
    </section>
  );
}
