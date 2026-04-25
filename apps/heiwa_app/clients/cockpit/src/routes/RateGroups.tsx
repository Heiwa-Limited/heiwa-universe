import type { JSX } from "solid-js";
import { For, Show } from "solid-js";
import { v1 } from "../lib/endpoints";
import { RemoteShell } from "../lib/resource";

export default function RateGroupsRoute(): JSX.Element {
  return (
    <section>
      <div class="hero compact">
        <p class="eyebrow">Rate groups</p>
        <h1>Live quota and priority</h1>
        <p class="lede">
          Grouped by vendor/auth. Priority decides which group the router
          prefers when multiple lanes can serve.
        </p>
      </div>

      <RemoteShell loader={() => v1.rateGroups()}>
        {(data) => (
          <Show
            when={data.rate_groups.length > 0}
            fallback={
              <div class="empty-state">
                <strong>No rate groups yet.</strong>
              </div>
            }
          >
            <div class="panels">
              <For each={data.rate_groups}>
                {(g) => (
                  <article class="panel">
                    <div class="status-card-head">
                      <h2>{g.group_id}</h2>
                      <span
                        class={`status-badge ${g.status === "healthy" ? "ok" : g.status === "throttled" ? "warn" : "fail"}`}
                      >
                        {g.status}
                      </span>
                    </div>
                    <p class="muted">
                      priority {g.priority} · {g.providers.join(", ")}
                    </p>
                    <Show when={g.notes}>
                      <p>{g.notes}</p>
                    </Show>
                    <pre class="mono-block">
                      {JSON.stringify(g.quota_state, null, 2)}
                    </pre>
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
