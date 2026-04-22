import type { JSX } from "solid-js";
import { For, Show } from "solid-js";
import { v1 } from "../lib/endpoints";
import { api } from "../lib/api";
import { RemoteShell } from "../lib/resource";

async function resolve(approvalId: string, action: "grant" | "deny", refetch: () => void): Promise<void> {
  await api.post(`/api/v1/approvals/${approvalId}/${action}`, {});
  refetch();
}

export default function ApprovalsRoute(): JSX.Element {
  return (
    <section>
      <div class="hero compact">
        <p class="eyebrow">Approvals</p>
        <h1>Pending operator decisions</h1>
        <p class="lede">Destructive actions and escalations pause here until the operator grants or denies.</p>
      </div>

      <RemoteShell loader={() => v1.approvals()}>
        {(data, refetch) => (
          <Show when={data.approvals.length > 0} fallback={<div class="empty-state"><strong>Nothing waiting.</strong><p class="muted">Missions don't need operator sign-off right now.</p></div>}>
            <div class="card-list">
              <For each={data.approvals}>
                {(a) => (
                  <article>
                    <div class="status-card-head">
                      <h3><code>{a.approval_id}</code></h3>
                      <span class={`status-badge ${a.risk_level === "critical" || a.risk_level === "high" ? "fail" : a.risk_level === "medium" ? "warn" : "ok"}`}>
                        {a.risk_level}
                      </span>
                    </div>
                    <p class="muted">mission <code>{a.mission_id}</code> · requested by {a.requested_by}</p>
                    <p>{a.summary}</p>
                    <p class="mono muted">requested {a.requested_at}{a.expires_at && ` · expires ${a.expires_at}`}</p>
                    <div class="hero-actions">
                      <button class="btn btn-solid" onClick={() => resolve(a.approval_id, "grant", refetch)}>Grant</button>
                      <button class="btn btn-outline" onClick={() => resolve(a.approval_id, "deny", refetch)}>Deny</button>
                    </div>
                  </article>
                )}
              </For>
            </div>
          </Show>
        )}
      </RemoteShell>
    </section>
  );
}
