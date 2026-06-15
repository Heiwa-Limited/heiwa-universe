import type { JSX } from "solid-js";
import { For, Show } from "solid-js";
import { api } from "../lib/api";
import { v1 } from "../lib/endpoints";
import { RemoteShell } from "../lib/resource";
import type { Approval, ApprovalsSummary } from "../lib/types";

interface ApprovalsBundle {
  approvals: Approval[];
  summary: ApprovalsSummary;
}

async function loadApprovalsBundle(): Promise<ApprovalsBundle> {
  const [missionResult, summary] = await Promise.all([
    v1.approvals(),
    v1.approvalsSummary(),
  ]);
  return { approvals: missionResult.approvals, summary };
}

function isApprovalEvent(msg: unknown): boolean {
  if (typeof msg !== "object" || msg === null) return false;
  const event = (msg as { event?: unknown }).event;
  return (
    event === "dispatch_request_appeared" ||
    event === "dispatch_request_decided"
  );
}

async function resolve(
  approvalId: string,
  action: "grant" | "deny",
  refetch: () => void,
): Promise<void> {
  await api.post(`/api/v1/approvals/${approvalId}/${action}`, {});
  refetch();
}

export default function ApprovalsRoute(): JSX.Element {
  return (
    <section>
      <div class="hero compact">
        <p class="eyebrow">Approvals</p>
        <h1>Pending operator decisions</h1>
        <p class="lede">
          Destructive actions and escalations pause here until the operator
          grants or denies.
        </p>
      </div>

      <RemoteShell
        loader={loadApprovalsBundle}
        liveEventFilter={isApprovalEvent}
      >
        {(data, refetch) => (
          <>
            <h2>Mission approvals</h2>
            <Show
              when={data.approvals.length > 0}
              fallback={
                <div class="empty-state">
                  <strong>Nothing waiting.</strong>
                  <p class="muted">
                    Missions don't need operator sign-off right now.
                  </p>
                </div>
              }
            >
              <div class="card-list">
                <For each={data.approvals}>
                  {(a) => (
                    <article>
                      <div class="status-card-head">
                        <h3>
                          <code>{a.approval_id}</code>
                        </h3>
                        <span
                          class={`status-badge ${a.risk_level === "critical" || a.risk_level === "high" ? "fail" : a.risk_level === "medium" ? "warn" : "ok"}`}
                        >
                          {a.risk_level}
                        </span>
                      </div>
                      <p class="muted">
                        mission <code>{a.mission_id}</code> · requested by{" "}
                        {a.requested_by}
                      </p>
                      <p>{a.summary}</p>
                      <p class="mono muted">
                        requested {a.requested_at}
                        {a.expires_at && ` · expires ${a.expires_at}`}
                      </p>
                      <div class="hero-actions">
                        <button
                          class="btn btn-solid"
                          onClick={() =>
                            resolve(a.approval_id, "grant", refetch)
                          }
                          type="button"
                        >
                          Grant
                        </button>
                        <button
                          class="btn btn-outline"
                          onClick={() =>
                            resolve(a.approval_id, "deny", refetch)
                          }
                          type="button"
                        >
                          Deny
                        </button>
                      </div>
                    </article>
                  )}
                </For>
              </div>
            </Show>

            <h2>Dispatch requests</h2>
            <p class="muted">
              Local dispatch v1 queue at{" "}
              <code>{data.summary.requests_dir}</code>. Decide with{" "}
              <code>heiwa approvals decide &lt;id&gt;</code>.
            </p>
            <Show
              when={data.summary.pending.length > 0}
              fallback={
                <div class="empty-state">
                  <strong>Queue is clear.</strong>
                  <p class="muted">No pending dispatch requests on file.</p>
                </div>
              }
            >
              <div class="card-list">
                <For each={data.summary.pending}>
                  {(req) => (
                    <article>
                      <div class="status-card-head">
                        <h3>
                          <code>{req.id}</code>
                        </h3>
                        <span class="status-badge warn">{req.risk}</span>
                      </div>
                      <p>
                        <strong>{req.action}</strong> →{" "}
                        <code>{req.target}</code>
                      </p>
                      <Show when={req.requested_at}>
                        <p class="mono muted">requested {req.requested_at}</p>
                      </Show>
                    </article>
                  )}
                </For>
              </div>
            </Show>
          </>
        )}
      </RemoteShell>
    </section>
  );
}
