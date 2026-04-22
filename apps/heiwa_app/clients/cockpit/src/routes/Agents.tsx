import type { JSX } from "solid-js";
import { For, Show } from "solid-js";
import { v1 } from "../lib/endpoints";
import { RemoteShell } from "../lib/resource";

export default function AgentsRoute(): JSX.Element {
  return (
    <section>
      <div class="hero compact">
        <p class="eyebrow">Agents</p>
        <h1>Subagent sessions</h1>
        <p class="lede">Parent sessions own their children. Kill, attach, or inspect from the CLI or from here.</p>
      </div>

      <RemoteShell loader={() => v1.agents()}>
        {(data) => (
          <Show when={data.agents.length > 0} fallback={<div class="empty-state"><strong>No subagents.</strong><p class="muted">Spawn one with <code>heiwa agent spawn</code>.</p></div>}>
            <div class="data-table-wrap">
              <table class="data-table">
                <thead>
                  <tr><th>Agent</th><th>Parent</th><th>Role</th><th>Status</th><th>Last event</th></tr>
                </thead>
                <tbody>
                  <For each={data.agents}>
                    {(a) => (
                      <tr>
                        <td><code>{a.agent_id}</code></td>
                        <td>{a.parent_id ? <code>{a.parent_id}</code> : <span class="muted">root</span>}</td>
                        <td>{a.role}</td>
                        <td><span class={`status-badge ${a.status === "running" || a.status === "attached" ? "ok" : a.status === "killed" ? "fail" : "warn"}`}>{a.status}</span></td>
                        <td class="mono">{a.last_event_at ?? "—"}</td>
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
