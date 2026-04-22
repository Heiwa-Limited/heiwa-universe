import type { JSX } from "solid-js";
import { For, Show } from "solid-js";
import { v1 } from "../lib/endpoints";
import { RemoteShell } from "../lib/resource";

export default function MemoryRoute(): JSX.Element {
  return (
    <section>
      <div class="hero compact">
        <p class="eyebrow">Memory</p>
        <h1>Durable operator memory</h1>
        <p class="lede">User, project, and session scopes. Heiwa's long-term memory lives here — separate from any model's working context.</p>
      </div>

      <RemoteShell loader={() => v1.memory()}>
        {(data) => (
          <Show when={data.entries.length > 0} fallback={<div class="empty-state"><strong>Memory is empty.</strong><p class="muted">Run <code>heiwa memory ingest</code> or save from a session.</p></div>}>
            <div class="card-list">
              <For each={data.entries}>
                {(e) => (
                  <article>
                    <div class="status-card-head">
                      <h3>{e.title}</h3>
                      <span class="pill">{e.scope}</span>
                    </div>
                    <p class="muted">{e.summary ?? "—"}</p>
                    <p class="mono muted">{e.source ?? "unknown source"} · updated {e.updated_at}</p>
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
