import type { JSX } from "solid-js";
import { For, Show } from "solid-js";
import { v1 } from "../lib/endpoints";
import { RemoteShell } from "../lib/resource";

export default function LiveRoute(): JSX.Element {
  return (
    <section>
      <div class="hero compact">
        <p class="eyebrow">Live</p>
        <h1>Streaming task activity</h1>
        <p class="lede">
          Current running work from the local runtime. This page will eventually
          layer <code>/ws/v1/events</code> on top; for now it reads running
          missions.
        </p>
      </div>

      <RemoteShell loader={() => v1.missions({ status: "running", limit: 50 })}>
        {(data) => (
          <Show
            when={data.missions.length > 0}
            fallback={
              <div class="empty-state">
                <strong>No live task activity.</strong>
                <p class="muted">
                  Running missions will appear here as the local runtime starts
                  streaming updates.
                </p>
              </div>
            }
          >
            <div class="card-list">
              <For each={data.missions}>
                {(mission) => (
                  <article>
                    <div class="status-card-head">
                      <h3>
                        <code>{mission.mission_id}</code>
                      </h3>
                      <span class="status-badge warn">{mission.status}</span>
                    </div>
                    <p>
                      <strong>Intent:</strong> {mission.intent_class ?? "—"}
                    </p>
                    <p>
                      <strong>Target:</strong>{" "}
                      {mission.target_model ?? mission.target_tool ?? "—"}
                    </p>
                    <p>{mission.summary ?? mission.prompt.slice(0, 140)}</p>
                    <p class="mono muted">updated {mission.updated_at}</p>
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
