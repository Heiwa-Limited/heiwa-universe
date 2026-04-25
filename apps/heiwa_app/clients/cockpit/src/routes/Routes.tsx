import type { JSX } from "solid-js";
import { For, Show } from "solid-js";
import { v1 } from "../lib/endpoints";
import { RemoteShell } from "../lib/resource";

export default function RoutesRoute(): JSX.Element {
  return (
    <section>
      <div class="hero compact">
        <p class="eyebrow">Routes</p>
        <h1>Routing config</h1>
        <p class="lede">
          Live route policy from <code>/api/v1/routes</code>. Fallbacks and
          offline capability stay explicit.
        </p>
      </div>

      <RemoteShell loader={() => v1.routes()}>
        {(data) => (
          <Show
            when={data.routes.length > 0}
            fallback={
              <div class="empty-state">
                <strong>No routes yet.</strong>
              </div>
            }
          >
            <div class="data-table-wrap">
              <table class="data-table">
                <thead>
                  <tr>
                    <th>Role</th>
                    <th>Provider</th>
                    <th>Model</th>
                    <th>Source</th>
                    <th>Fallbacks</th>
                    <th>Offline</th>
                  </tr>
                </thead>
                <tbody>
                  <For each={data.routes}>
                    {(route) => (
                      <tr>
                        <td>{route.role}</td>
                        <td>{route.provider}</td>
                        <td>
                          <code>{route.model}</code>
                        </td>
                        <td>{route.source}</td>
                        <td>{route.fallbacks.join(", ") || "—"}</td>
                        <td>{route.offline_capable ? "yes" : "no"}</td>
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
