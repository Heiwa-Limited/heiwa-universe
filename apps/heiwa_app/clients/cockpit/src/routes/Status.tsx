import type { JSX } from "solid-js";
import { For, Show } from "solid-js";
import { v1 } from "../lib/endpoints";
import { RemoteShell } from "../lib/resource";

export default function StatusRoute(): JSX.Element {
  return (
    <section>
      <div class="hero compact">
        <p class="eyebrow">Status</p>
        <h1>Runtime health</h1>
        <p class="lede">
          Read from <code>/status/health</code>. Safe for unauthenticated local
          probes.
        </p>
      </div>

      <RemoteShell loader={() => v1.health()}>
        {(data) => (
          <div class="kpi-grid">
            <div class="kpi">
              <span>Status</span>
              <strong>
                <span
                  class={`status-badge ${data.status === "ok" ? "ok" : data.status === "degraded" ? "warn" : "fail"}`}
                >
                  {data.status}
                </span>
              </strong>
            </div>
            <div class="kpi">
              <span>Runtime</span>
              <strong>{data.runtime_version}</strong>
            </div>
            <div class="kpi">
              <span>Started</span>
              <strong class="mono">{data.started_at}</strong>
            </div>
            <div class="kpi">
              <span>Notes</span>
              <Show when={data.notes.length > 0} fallback={<strong>—</strong>}>
                <ul>
                  <For each={data.notes}>{(n) => <li>{n}</li>}</For>
                </ul>
              </Show>
            </div>
          </div>
        )}
      </RemoteShell>
    </section>
  );
}
