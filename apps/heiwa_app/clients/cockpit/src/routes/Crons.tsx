import type { JSX } from "solid-js";
import { For, Show } from "solid-js";
import { v1 } from "../lib/endpoints";
import { RemoteShell } from "../lib/resource";

export default function CronsRoute(): JSX.Element {
  return (
    <section>
      <div class="hero compact">
        <p class="eyebrow">Crons</p>
        <h1>Scheduled routines</h1>
        <p class="lede">
          Jobs that run on cron or rrule. Managed under the local scheduler, not
          an external service.
        </p>
      </div>

      <RemoteShell loader={() => v1.crons()}>
        {(data) => (
          <Show
            when={data.crons.length > 0}
            fallback={
              <div class="empty-state">
                <strong>No scheduled jobs.</strong>
                <p class="muted">
                  Add one with <code>heiwa cron add</code>.
                </p>
              </div>
            }
          >
            <div class="data-table-wrap">
              <table class="data-table">
                <thead>
                  <tr>
                    <th>Name</th>
                    <th>Schedule</th>
                    <th>Status</th>
                    <th>Last run</th>
                    <th>Next run</th>
                  </tr>
                </thead>
                <tbody>
                  <For each={data.crons}>
                    {(c) => (
                      <tr>
                        <td>
                          <strong>{c.name}</strong>
                          <div class="muted mono">{c.job_id}</div>
                        </td>
                        <td class="mono">{c.schedule}</td>
                        <td>
                          <span
                            class={`status-badge ${c.status === "enabled" || c.status === "running" ? "ok" : "warn"}`}
                          >
                            {c.status}
                          </span>
                        </td>
                        <td class="mono">{c.last_run_at ?? "—"}</td>
                        <td class="mono">{c.next_run_at ?? "—"}</td>
                      </tr>
                    )}
                  </For>
                </tbody>
              </table>
            </div>
          </Show>
        )}
      </RemoteShell>
    </section>
  );
}
