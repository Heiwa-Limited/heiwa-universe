import type { JSX } from "solid-js";
import { For, Show } from "solid-js";
import { v1 } from "../lib/endpoints";
import { RemoteShell } from "../lib/resource";

export default function MissionsRoute(): JSX.Element {
  return (
    <section>
      <div class="hero compact">
        <p class="eyebrow">Missions</p>
        <h1>Active and recent work</h1>
        <p class="lede">
          Running, paused, and recently completed missions. Streams live via{" "}
          <code>/ws/v1/events</code>.
        </p>
      </div>

      <RemoteShell loader={() => v1.missions({ limit: 50 })}>
        {(data) => (
          <Show
            when={data.missions.length > 0}
            fallback={
              <div class="empty-state">
                <strong>No missions yet.</strong>
                <p class="muted">
                  Run <code>heiwa run &lt;prompt&gt;</code> to create one.
                </p>
              </div>
            }
          >
            <div class="data-table-wrap">
              <table class="data-table">
                <thead>
                  <tr>
                    <th>Mission</th>
                    <th>Status</th>
                    <th>Intent</th>
                    <th>Target</th>
                    <th>Updated</th>
                  </tr>
                </thead>
                <tbody>
                  <For each={data.missions}>
                    {(m) => (
                      <tr>
                        <td>
                          <code>{m.mission_id}</code>
                          <div class="muted">
                            {m.summary ?? m.prompt.slice(0, 80)}
                          </div>
                        </td>
                        <td>
                          <span
                            class={`status-badge ${m.status === "done" ? "ok" : m.status === "failed" ? "fail" : "warn"}`}
                          >
                            {m.status}
                          </span>
                        </td>
                        <td>{m.intent_class ?? "—"}</td>
                        <td>{m.target_model ?? m.target_tool ?? "—"}</td>
                        <td class="mono">{m.updated_at}</td>
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
