import type { JSX } from "solid-js";
import { For, Show } from "solid-js";
import { v1 } from "../lib/endpoints";
import { providers, type Lane } from "../lib/providers";
import { RemoteShell } from "../lib/resource";

function providerMeta(providerId: string): { label: string; maturity: string | null; lanes: Lane[] } {
  const match = providers.providers.find((provider) => provider.id === providerId);
  return {
    label: match?.name ?? providerId,
    maturity: match ? providers.maturity[match.maturity].label : null,
    lanes: match?.lanes ?? [],
  };
}

export default function ConnectionsRoute(): JSX.Element {
  return (
    <section>
      <div class="hero compact">
        <p class="eyebrow">Connections</p>
        <h1>Provider connections and readiness</h1>
        <p class="lede">
          Live provider state from the local runtime, joined against the shared provider catalog so marketing and cockpit stay aligned.
        </p>
      </div>

      <RemoteShell loader={() => v1.providers()}>
        {(data) => (
          <Show
            when={data.providers.length > 0}
            fallback={
              <div class="empty-state">
                <strong>No provider metadata yet.</strong>
                <p class="muted">Link a provider with <code>heiwa providers link &lt;id&gt;</code>.</p>
              </div>
            }
          >
            <div class="panels">
              <For each={data.providers}>
                {(provider) => {
                  const meta = providerMeta(provider.provider_id);
                  return (
                    <article class="panel">
                      <div class="status-card-head">
                        <h2>{meta.label}</h2>
                        <span class={`status-badge ${provider.status === "connected" ? "ok" : provider.status === "error" ? "fail" : "warn"}`}>
                          {provider.status}
                        </span>
                      </div>
                      <p class="muted">{provider.auth_kind} · {provider.rate_group ?? "no rate group"}</p>
                      <p><strong>Default model:</strong> <code>{provider.default_model ?? "—"}</code></p>
                      <p><strong>Catalog maturity:</strong> {meta.maturity ?? "Unknown"}</p>
                      <Show when={meta.lanes.length > 0}>
                        <div class="operator-meta">
                          <For each={meta.lanes}>{(lane) => <span class="pill">{providers.lanes[lane].short}</span>}</For>
                        </div>
                      </Show>
                      <p class="mono muted">validated {provider.last_validated_at ?? "never"}</p>
                      <Show when={provider.last_error}>
                        <p>{provider.last_error}</p>
                      </Show>
                    </article>
                  );
                }}
              </For>
            </div>
          </Show>
        )}
      </RemoteShell>
    </section>
  );
}
