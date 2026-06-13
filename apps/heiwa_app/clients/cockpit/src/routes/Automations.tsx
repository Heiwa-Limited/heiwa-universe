import type { JSX } from "solid-js";
import { For, Show } from "solid-js";
import { v1 } from "../lib/endpoints";
import { RemoteShell } from "../lib/resource";
import { EmptyState, PageHero, Panel, StatusBadge } from "../lib/ui";
import type { Automation } from "../lib/types";

function triggerSummary(trigger: Automation["trigger_config"]): string {
  if (!trigger) return "manual";
  const value = trigger as Record<string, unknown>;
  const kind = typeof value.type === "string" ? value.type : "unknown";
  if (kind === "cron") {
    const schedule = typeof value.schedule === "string" ? value.schedule : "?";
    const timezone = typeof value.timezone === "string" ? value.timezone : null;
    return `cron ${schedule}${timezone ? ` (${timezone})` : ""}`;
  }
  if (kind === "file_watch") {
    const paths = Array.isArray(value.paths) ? value.paths.join(", ") : "?";
    const events = Array.isArray(value.events) ? value.events.join(",") : "?";
    const pattern = typeof value.pattern === "string" ? value.pattern : "*";
    return `watch ${paths} · ${events} · ${pattern}`;
  }
  return kind;
}

function executionTone(status: string): "ok" | "warn" | "fail" {
  if (status === "completed") return "ok";
  if (status === "failed" || status === "cancelled") return "fail";
  return "warn";
}

export default function AutomationsRoute(): JSX.Element {
  return (
    <section>
      <PageHero
        eyebrow="Automations"
        title="Background work primitives"
        lede="Local-only automation definitions, scheduler state, execution queue receipts, and manual trigger visibility. Daemon launchd comes after this read model stays proven."
      />

      <RemoteShell loader={() => v1.automations()}>
        {(data) => (
          <>
            <div class="panels">
              <Panel
                title="Local automation store"
                status={`${data.automation_count} total`}
                tone={data.automation_count > 0 ? "ok" : "warn"}
              >
                <p class="muted">
                  State: <code>{data.state_dir}</code>
                </p>
                <p class="muted">
                  DB: <code>{data.db_path ?? "not initialized"}</code>
                </p>
                <Show when={data.error}>
                  {(error) => <p class="repl-error">{error()}</p>}
                </Show>
              </Panel>

              <Panel
                title="Scheduler"
                status={`${data.active_count} active`}
                tone={data.active_count > 0 ? "ok" : "warn"}
              >
                <ul>
                  <li>
                    <strong>{data.scheduler.active_cron}</strong> active cron trigger(s)
                  </li>
                  <li>
                    <strong>{data.scheduler.active_file_watch}</strong> active file watcher(s)
                  </li>
                  <li>
                    next run: <code>{data.scheduler.next_scheduled_at ?? "none"}</code>
                  </li>
                </ul>
              </Panel>
            </div>

            <h2>Definitions</h2>
            <Show
              when={data.automations.length > 0}
              fallback={
                <EmptyState title="No automations yet.">
                  <p class="muted">
                    Create one with <code>heiwa auto create --name NAME --prompt PROMPT --cron '0 9 * * *' --active</code>.
                  </p>
                </EmptyState>
              }
            >
              <div class="data-table-wrap">
                <table class="data-table">
                  <thead>
                    <tr>
                      <th>Name</th>
                      <th>Trigger</th>
                      <th>Status</th>
                      <th>Next</th>
                      <th>Limits</th>
                    </tr>
                  </thead>
                  <tbody>
                    <For each={data.automations}>
                      {(automation) => (
                        <tr>
                          <td>
                            <strong>{automation.name}</strong>
                            <div class="muted mono">{automation.id}</div>
                            <p class="muted">{automation.prompt}</p>
                          </td>
                          <td class="mono">{triggerSummary(automation.trigger_config)}</td>
                          <td>
                            <StatusBadge status={automation.status} tone={automation.status === "active" ? "ok" : "warn"} />
                          </td>
                          <td class="mono">{automation.next_scheduled_at ?? "—"}</td>
                          <td class="mono">
                            hour {automation.max_executions_per_hour ?? "∞"} · day{" "}
                            {automation.max_executions_per_day ?? "∞"}
                          </td>
                        </tr>
                      )}
                    </For>
                  </tbody>
                </table>
              </div>
            </Show>

            <h2>Recent executions</h2>
            <Show
              when={data.recent_executions.length > 0}
              fallback={
                <EmptyState title="No executions queued yet.">
                  <p class="muted">
                    Queue one with <code>heiwa auto trigger &lt;automation-id&gt; --json</code> or run <code>heiwa auto tick --json</code>.
                  </p>
                </EmptyState>
              }
            >
              <div class="data-table-wrap">
                <table class="data-table">
                  <thead>
                    <tr>
                      <th>Execution</th>
                      <th>Automation</th>
                      <th>Status</th>
                      <th>Created</th>
                      <th>Completed</th>
                    </tr>
                  </thead>
                  <tbody>
                    <For each={data.recent_executions}>
                      {(execution) => (
                        <tr>
                          <td class="mono">{execution.id}</td>
                          <td class="mono">{execution.automation_id}</td>
                          <td>
                            <StatusBadge status={execution.status} tone={executionTone(execution.status)} />
                            <Show when={execution.error_message}>
                              {(message) => <p class="repl-error">{message()}</p>}
                            </Show>
                          </td>
                          <td class="mono">{execution.created_at}</td>
                          <td class="mono">{execution.completed_at ?? "—"}</td>
                        </tr>
                      )}
                    </For>
                  </tbody>
                </table>
              </div>
            </Show>
          </>
        )}
      </RemoteShell>
    </section>
  );
}
