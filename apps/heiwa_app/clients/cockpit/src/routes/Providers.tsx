import type { JSX } from "solid-js";
import { For } from "solid-js";
import { providers, type Provider } from "../lib/providers";

function laneBadges(p: Provider): JSX.Element {
  return (
    <div class="operator-meta">
      <For each={p.lanes}>
        {(lane) => <span class="pill">{providers.lanes[lane].short}</span>}
      </For>
    </div>
  );
}

export default function ProvidersRoute(): JSX.Element {
  return (
    <section>
      <div class="hero compact">
        <p class="eyebrow">Providers</p>
        <h1>Connected lanes</h1>
        <p class="lede">
          OAuth CLIs, API keys, and local runtimes under one cockpit. Maturity shown honestly by surface.
        </p>
      </div>
      <div class="panels">
        <For each={providers.providers}>
          {(p) => (
            <article class="panel">
              <div class="status-card-head">
                <h2>{p.name}</h2>
                <span class={`status-badge ${p.maturity === "stable" ? "ok" : p.maturity === "planned" ? "fail" : "warn"}`}>
                  {providers.maturity[p.maturity].label}
                </span>
              </div>
              <p class="muted">{p.vendor}</p>
              {p.description && <p>{p.description}</p>}
              {laneBadges(p)}
              {p.subscriptions && p.subscriptions.length > 0 && (
                <p class="mono">{p.subscriptions.join(" · ")}</p>
              )}
            </article>
          )}
        </For>
      </div>
    </section>
  );
}
