import type { JSX } from "solid-js";
import { For, Show } from "solid-js";
import { v1 } from "../lib/endpoints";
import { RemoteShell } from "../lib/resource";

export default function TracesRoute(): JSX.Element {
  return (
    <section>
      <div class="hero compact">
        <p class="eyebrow">Traces</p>
        <h1>Evidence and receipts</h1>
        <p class="lede">
          Per-mission route decisions, receipts, and artifacts. Drill in for the
          full record.
        </p>
      </div>

      <RemoteShell loader={() => v1.traces()}>
        {(data) => (
          <Show
            when={data.traces.length > 0}
            fallback={
              <div class="empty-state">
                <strong>No traces yet.</strong>
                <p class="muted">Traces accumulate as missions complete.</p>
              </div>
            }
          >
            <div class="card-list">
              <For each={data.traces}>
                {(t) => (
                  <article>
                    <div class="status-card-head">
                      <h3>
                        <code>{t.trace_id}</code>
                      </h3>
                      <span class="pill">
                        {t.route.role} · {t.route.provider}/{t.route.model}
                      </span>
                    </div>
                    <p class="muted">
                      mission <code>{t.mission_id}</code> · session{" "}
                      <code>{t.session_id}</code>
                    </p>
                    <p class="mono muted">
                      {t.started_at}
                      {t.ended_at && ` → ${t.ended_at}`}
                    </p>
                    <p>
                      {t.receipts.length} receipt(s) · {t.artifacts.length}{" "}
                      artifact(s)
                    </p>
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
