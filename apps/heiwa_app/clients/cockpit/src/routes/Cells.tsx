import type { JSX } from "solid-js";
import { For, Show } from "solid-js";
import { v1 } from "../lib/endpoints";
import { RemoteShell } from "../lib/resource";

export default function CellsRoute(): JSX.Element {
  return (
    <section>
      <div class="hero compact">
        <p class="eyebrow">Cells</p>
        <h1>Catalog</h1>
        <p class="lede">Read-only catalog of available cell definitions.</p>
      </div>

      <RemoteShell loader={() => v1.cells()}>
        {(data) => (
          <Show when={data.cells.length > 0} fallback={<div class="empty-state"><strong>Empty catalog.</strong></div>}>
            <div class="card-list">
              <For each={data.cells}>
                {(c) => (
                  <article>
                    <div class="status-card-head">
                      <h3>{c.label}</h3>
                      <span class="pill">{c.category}</span>
                    </div>
                    <p class="mono muted">{c.id}</p>
                    <Show when={c.description}><p>{c.description}</p></Show>
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
