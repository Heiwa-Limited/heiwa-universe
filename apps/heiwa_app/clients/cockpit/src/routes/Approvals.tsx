import type { JSX } from "solid-js";
import { createSignal, For, Show } from "solid-js";
import { api } from "../lib/api";
import { v1 } from "../lib/endpoints";
import { RemoteShell } from "../lib/resource";
import type { ApprovalsSummary } from "../lib/types";

interface ApprovalsBundle {
  summary: ApprovalsSummary;
}

async function loadApprovalsBundle(): Promise<ApprovalsBundle> {
  return { summary: await v1.approvalsSummary() };
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
  action: "approve" | "deny",
  refetch: () => void,
): Promise<void> {
  await api.post(`/api/v1/approvals/${approvalId}/${action}`, {});
  refetch();
}

export default function ApprovalsRoute(): JSX.Element {
  const [deciding, setDeciding] = createSignal<string | null>(null);
  const [decisionError, setDecisionError] = createSignal<string | null>(null);

  async function decide(
    id: string,
    action: "approve" | "deny",
    refetch: () => void,
  ): Promise<void> {
    if (deciding()) return;
    setDeciding(id);
    setDecisionError(null);
    try {
      await resolve(id, action, refetch);
    } catch (cause) {
      setDecisionError(
        cause instanceof Error
          ? cause.message
          : ((cause as { message?: string }).message ?? String(cause)),
      );
    } finally {
      setDeciding(null);
    }
  }

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
            <h2>Pending decisions</h2>
            <p class="muted">
              Each card is one local request. Approve or deny it here; the CLI
              uses the same immutable executor.
            </p>
            <Show when={decisionError()}>
              {(message) => <p class="repl-error">{message()}</p>}
            </Show>
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
                      <div class="hero-actions">
                        <button
                          class="btn btn-solid"
                          disabled={deciding() !== null}
                          onClick={() =>
                            void decide(req.id, "approve", refetch)
                          }
                          type="button"
                        >
                          Approve
                        </button>
                        <button
                          class="btn btn-outline"
                          disabled={deciding() !== null}
                          onClick={() => void decide(req.id, "deny", refetch)}
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
          </>
        )}
      </RemoteShell>
    </section>
  );
}
