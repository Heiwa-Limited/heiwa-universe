import { createSignal, For, Show } from "solid-js";
import { useApp } from "../../state/app";
import type { SurfaceModule } from "../types";
import "./approvals.css";

function ApprovalsSurface() {
  const app = useApp();
  const [busy, setBusy] = createSignal<string | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const pending = () => app.runtime.approvals()?.pending ?? [];

  async function decide(id: string, approve: boolean): Promise<void> {
    if (busy()) return;
    setBusy(id);
    setError(null);
    try {
      await app.runtime.decideApproval(id, approve);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(null);
    }
  }

  return (
    <div class="view approvals-view">
      <header class="surface-heading">
        <div>
          <p class="quiet">Governed side effects</p>
          <h2>Pending decisions</h2>
        </div>
        <strong>{pending().length}</strong>
      </header>

      <Show when={error()}>
        {(message) => <p class="surface-error">{message()}</p>}
      </Show>

      <Show
        when={pending().length > 0}
        fallback={
          <section class="panel approval-empty">
            <strong>Nothing waiting.</strong>
            <p class="quiet">No user decision is required right now.</p>
          </section>
        }
      >
        <div class="approval-list">
          <For each={pending()}>
            {(request) => (
              <article class="panel approval-card">
                <header>
                  <span>{request.action}</span>
                  <strong>{request.risk}</strong>
                </header>
                <code>{request.target}</code>
                <p class="quiet">
                  {request.id}
                  {request.requested_at ? ` · ${request.requested_at}` : ""}
                </p>
                <div class="approval-actions">
                  <button
                    class="small-action approval-approve"
                    aria-label={`Approve ${request.id}`}
                    disabled={busy() !== null}
                    onClick={() => void decide(request.id, true)}
                  >
                    Approve
                  </button>
                  <button
                    class="small-action"
                    aria-label={`Deny ${request.id}`}
                    disabled={busy() !== null}
                    onClick={() => void decide(request.id, false)}
                  >
                    Deny
                  </button>
                </div>
              </article>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
}

export const approvalsSurface: SurfaceModule = {
  id: "approvals",
  label: "Approvals",
  glyph: "✓",
  caption: "approval queue",
  Component: ApprovalsSurface,
  preview: (app) => ({
    title: "Approvals",
    lines: [
      `${app.runtime.approvals()?.pending_count ?? 0} pending`,
      "effects run only after your decision",
    ],
  }),
  refresh: (app) => app.runtime.loadApprovals(),
};
