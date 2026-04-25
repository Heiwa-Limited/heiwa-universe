import type { JSX } from "solid-js";
import { For, Show } from "solid-js";
import { v1 } from "../lib/endpoints";
import { RemoteShell } from "../lib/resource";

export default function HistoryRoute(): JSX.Element {
  return (
    <section>
      <div class="hero compact">
        <p class="eyebrow">History</p>
        <h1>Sessions, runs, and artifacts</h1>
        <p class="lede">
          Recent operator activity. Durable memory lives under{" "}
          <code>/memory</code>; this is the recent-session view.
        </p>
      </div>

      <RemoteShell loader={() => v1.history()}>
        {(data) => (
          <div class="operator-grid">
            <article class="panel">
              <h2>Recent sessions</h2>
              <Show
                when={data.sessions.length > 0}
                fallback={<p class="muted">No sessions yet.</p>}
              >
                <ul>
                  <For each={data.sessions}>
                    {(s) => (
                      <li>
                        <code>{s.id}</code> ·{" "}
                        <span class="mono muted">{s.started_at}</span> ·{" "}
                        {s.mission_count} missions
                      </li>
                    )}
                  </For>
                </ul>
              </Show>
            </article>
            <article class="panel">
              <h2>Recent runs</h2>
              <Show
                when={data.recent_runs.length > 0}
                fallback={<p class="muted">No runs yet.</p>}
              >
                <ul>
                  <For each={data.recent_runs}>
                    {(r) => (
                      <li>
                        <code>{r.mission_id}</code> ·{" "}
                        <span
                          class={`status-badge ${r.status === "done" ? "ok" : "warn"}`}
                        >
                          {r.status}
                        </span>{" "}
                        · <span class="muted">{r.summary ?? "—"}</span>
                      </li>
                    )}
                  </For>
                </ul>
              </Show>
            </article>
            <article class="panel panel-full">
              <h2>Artifacts</h2>
              <Show
                when={data.artifacts.length > 0}
                fallback={<p class="muted">No artifacts yet.</p>}
              >
                <ul>
                  <For each={data.artifacts}>
                    {(a) => (
                      <li>
                        <code>{a.id}</code> ·{" "}
                        <span class="muted">{a.kind}</span> · {a.label}
                      </li>
                    )}
                  </For>
                </ul>
              </Show>
            </article>
          </div>
        )}
      </RemoteShell>
    </section>
  );
}
