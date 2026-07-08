import type { JSX } from "solid-js";
import { For, Show } from "solid-js";
import { v1 } from "../lib/endpoints";
import { RemoteShell } from "../lib/resource";
import { EmptyState, PageHero, Panel, StatusBadge } from "../lib/ui";

export default function MailRoute(): JSX.Element {
  return (
    <section>
      <PageHero
        eyebrow="Mail"
        title="Communications cockpit"
        lede="Email becomes a governed intake lane: scan, report, summarize, draft, and approval-send from Heiwa.app without hidden outbound automation."
      />

      <RemoteShell loader={() => v1.mailSummary()}>
        {(data) => (
          <>
            <div class="panels">
              <Panel
                title="Priority scan"
                status={
                  data.snapshot.present
                    ? `${data.counts.priority} rows`
                    : "no snapshot"
                }
                tone={data.snapshot.present ? "ok" : "warn"}
              >
                <Show
                  when={data.priority.length > 0}
                  fallback={
                    <EmptyState title="No mail metadata rows yet.">
                      <p class="muted">
                        Populate <code>{data.snapshot.path}</code> with{" "}
                        <code>heiwa mail scan</code> (Apple Mail or Gmail
                        lanes). Bodies are never read; each scan writes a
                        receipt.
                      </p>
                    </EmptyState>
                  }
                >
                  <ul>
                    <For each={data.priority}>
                      {(row) => (
                        <li>
                          <strong>{row.subject ?? "(no subject)"}</strong>{" "}
                          <StatusBadge status={row.action} />
                          <Show when={row.unread}>
                            <StatusBadge status="unread" tone="warn" />
                          </Show>
                          <p class="muted">
                            {row.sender ?? "unknown sender"} ·{" "}
                            {row.date ?? "undated"} · score {row.score}
                          </p>
                        </li>
                      )}
                    </For>
                  </ul>
                </Show>
                <p class="mono muted">
                  policy: {data.policy} · accounts detected:{" "}
                  {data.counts.accounts}
                </p>
              </Panel>

              <Panel title="Reply safety" status="approval-first" tone="ok">
                <ol>
                  <li>Classify thread and sensitivity (local lane first).</li>
                  <li>Draft response locally or through the routing matrix.</li>
                  <li>
                    Show recipients, subject, body, attachments, source thread,
                    and risk.
                  </li>
                  <li>
                    Send only after approval; write a receipt with external IDs
                    and body hash.
                  </li>
                </ol>
              </Panel>
            </div>

            <div class="card-list">
              <For each={data.lanes}>
                {(lane) => (
                  <article>
                    <div class="status-card-head">
                      <h3>{lane.name}</h3>
                      <StatusBadge status={lane.status} />
                    </div>
                    <p>
                      <strong>Read:</strong> {lane.read}
                    </p>
                    <p>
                      <strong>Reply:</strong> {lane.reply}
                    </p>
                    <p class="muted">
                      <strong>Guardrail:</strong> {lane.guardrail}
                    </p>
                  </article>
                )}
              </For>
            </div>
          </>
        )}
      </RemoteShell>
    </section>
  );
}
