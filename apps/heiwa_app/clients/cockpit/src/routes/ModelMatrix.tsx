import type { JSX } from "solid-js";
import { For, Show, createSignal } from "solid-js";
import { v1 } from "../lib/endpoints";
import { RemoteShell } from "../lib/resource";
import { PageHero, Panel, StatusBadge, statusTone } from "../lib/ui";
import type { RoutePreview } from "../lib/types";

const ROLE_COPY: Record<string, string> = {
  chat: "conversation, summaries, life briefs",
  build: "coding agents, repo work, implementation loops",
  research: "long reasoning, comparisons, external synthesis",
  audit: "review, verification, risk checks",
};

function RoutePreviewTester(): JSX.Element {
  const [prompt, setPrompt] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [preview, setPreview] = createSignal<RoutePreview | null>(null);
  const [error, setError] = createSignal<string | null>(null);

  async function submit(event: Event): Promise<void> {
    event.preventDefault();
    const text = prompt().trim();
    if (!text || busy()) return;
    setBusy(true);
    setError(null);
    try {
      setPreview(await v1.routePreview(text));
    } catch (err) {
      const message =
        err instanceof Error
          ? err.message
          : ((err as { message?: string }).message ?? String(err));
      setError(message);
      setPreview(null);
    } finally {
      setBusy(false);
    }
  }

  return (
    <Panel title="Route preview" status="no execution" tone="ok">
      <p class="muted">
        Ask DREX where a task would route right now — capability, quota,
        privacy, and cost decide; no model runs.
      </p>
      <form class="hold-form-row" onSubmit={(event) => void submit(event)}>
        <input
          type="text"
          placeholder="e.g. refactor the auth module and add tests"
          value={prompt()}
          onInput={(event) => setPrompt(event.currentTarget.value)}
        />
        <button
          type="submit"
          class="btn btn-outline"
          disabled={busy() || !prompt().trim()}
        >
          {busy() ? "Routing…" : "Preview route"}
        </button>
      </form>
      <Show when={error()}>
        {(message) => <p class="repl-error">{message()}</p>}
      </Show>
      <Show when={preview()}>
        {(result) => (
          <div class="route-preview-result">
            <p>
              <StatusBadge status={result().mode} />{" "}
              <Show when={result().provider}>
                <strong>
                  {result().provider}/{result().model}
                </strong>{" "}
                <span class="muted">
                  intent {result().intent} · rate group {result().rate_group}
                </span>
              </Show>
              <Show when={result().mode === "deterministic"}>
                <span class="muted">
                  answered locally without any model: “{result().response}”
                </span>
              </Show>
              <Show when={result().error}>
                <span class="muted">{result().error}</span>
              </Show>
            </p>
            <Show when={result().quota.length > 0}>
              <div class="trace-panel compact">
                <span class="trace-header">Quota lanes</span>
                <div class="trace-log-viewport">
                  <For each={result().quota}>
                    {(line) => <div class="trace-log-line system">{line}</div>}
                  </For>
                </div>
              </div>
            </Show>
          </div>
        )}
      </Show>
    </Panel>
  );
}

export default function ModelMatrixRoute(): JSX.Element {
  return (
    <section>
      <PageHero
        eyebrow="Model Matrix"
        title="Routing intelligence, not model picking"
        lede="Heiwa chooses provider/model/device lanes through capability probes, evals, quota, privacy, cost, and evidence. Users see the reason and receipt, not a model dropdown."
      />

      <RemoteShell loader={() => v1.routes()}>
        {(data) => (
          <>
            <div class="panels">
              <RoutePreviewTester />

              <Panel
                title="Live route table"
                status={
                  data.routes.some((route) => route.source === "drex_live")
                    ? "drex_live"
                    : "degraded"
                }
                tone={
                  data.routes.some((route) => route.source === "drex_live")
                    ? "ok"
                    : "warn"
                }
              >
                <p class="muted">
                  What DREX would pick per intent right now, from the cached
                  account registry. No dropdown — pin a provider per turn via
                  the REPL if you must override.
                </p>
                <div class="data-table-wrap">
                  <table class="data-table">
                    <thead>
                      <tr>
                        <th>Intent</th>
                        <th>Lane today</th>
                        <th>Fallbacks</th>
                        <th>Offline</th>
                      </tr>
                    </thead>
                    <tbody>
                      <For each={data.routes}>
                        {(route) => (
                          <tr>
                            <td>
                              <strong>{route.role}</strong>
                              <p class="muted">{ROLE_COPY[route.role] ?? ""}</p>
                            </td>
                            <td>
                              <Show
                                when={route.provider}
                                fallback={<StatusBadge status="unavailable" />}
                              >
                                <code>
                                  {route.provider}/{route.model}
                                </code>
                                <Show when={route.rate_group}>
                                  <p class="muted">{route.rate_group}</p>
                                </Show>
                              </Show>
                            </td>
                            <td>{route.fallbacks.join(" → ") || "—"}</td>
                            <td>
                              <span
                                class={`status-badge ${statusTone(route.offline_capable ? "ok" : "planned")}`}
                              >
                                {route.offline_capable ? "yes" : "no"}
                              </span>
                            </td>
                          </tr>
                        )}
                      </For>
                    </tbody>
                  </table>
                </div>
              </Panel>
            </div>
          </>
        )}
      </RemoteShell>
    </section>
  );
}
