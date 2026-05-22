import type { JSX } from "solid-js";
import { For, Show } from "solid-js";
import { v1 } from "../lib/endpoints";
import { RemoteShell } from "../lib/resource";
import type { HookProvider } from "../lib/types";

function statusClass(status: string): string {
  if (status === "active" || status === "delegated") return "ok";
  if (status === "degraded") return "warn";
  return "fail";
}

function commandState(provider: HookProvider): string {
  const commands = provider.events.flatMap((event) => event.hooks);
  if (commands.length === 0) return "none";
  const missing = commands.filter(
    (hook) => hook.command_exists === false,
  ).length;
  return missing === 0 ? "all present" : `${missing} missing`;
}

export default function HooksRoute(): JSX.Element {
  return (
    <section>
      <div class="hero compact">
        <p class="eyebrow">Hooks</p>
        <h1>Runtime guardrails inside the cockpit</h1>
        <p class="lede">
          Live home hook posture from <code>~/.claude</code>,{" "}
          <code>~/.gemini</code>, <code>~/.codex</code>, and{" "}
          <code>~/.heiwa</code>. Provider APIs stay provider-owned; Heiwa reads,
          explains, and hardens the local boundary.
        </p>
      </div>

      <RemoteShell loader={() => v1.hooks()}>
        {(data) => (
          <>
            <div class="kpi-grid">
              <div class="kpi">
                <span>Source</span>
                <strong>{data.summary.source}</strong>
              </div>
              <div class="kpi">
                <span>Active providers</span>
                <strong>{data.summary.active}</strong>
              </div>
              <div class="kpi">
                <span>Hook events</span>
                <strong>{data.summary.events}</strong>
              </div>
              <div class="kpi">
                <span>Commands</span>
                <strong>{data.summary.commands}</strong>
              </div>
            </div>

            <div class="panels">
              <For each={data.providers}>
                {(provider) => (
                  <article class="panel">
                    <div class="status-card-head">
                      <h2>{provider.display_name}</h2>
                      <span
                        class={`status-badge ${statusClass(provider.status)}`}
                      >
                        {provider.status}
                      </span>
                    </div>
                    <p class="mono">{provider.config_path}</p>
                    <div class="domain-kv">
                      <div>
                        <span>Generated config</span>
                        <strong>{provider.generated_config_status}</strong>
                      </div>
                      <div>
                        <span>Command files</span>
                        <strong>{commandState(provider)}</strong>
                      </div>
                      <div>
                        <span>Audit</span>
                        <strong class="mono">
                          {provider.audit_file ?? "none"}
                        </strong>
                      </div>
                    </div>
                    <Show when={provider.notes.length > 0}>
                      <ul>
                        <For each={provider.notes}>
                          {(note) => <li>{note}</li>}
                        </For>
                      </ul>
                    </Show>
                  </article>
                )}
              </For>
            </div>

            <div class="data-table-wrap" style={{ "margin-top": "1rem" }}>
              <table class="data-table">
                <thead>
                  <tr>
                    <th>Provider</th>
                    <th>Event</th>
                    <th>Matcher</th>
                    <th>Hook</th>
                    <th>Command</th>
                  </tr>
                </thead>
                <tbody>
                  <For each={data.providers}>
                    {(provider) => (
                      <For each={provider.events}>
                        {(event) => (
                          <For each={event.hooks}>
                            {(hook) => (
                              <tr>
                                <td>{provider.display_name}</td>
                                <td>
                                  <code>{event.event}</code>
                                </td>
                                <td>
                                  <code>{event.matcher}</code>
                                </td>
                                <td>{hook.name ?? hook.kind ?? "command"}</td>
                                <td class="mono">
                                  {hook.command_path ?? hook.command}
                                </td>
                              </tr>
                            )}
                          </For>
                        )}
                      </For>
                    )}
                  </For>
                </tbody>
              </table>
            </div>
          </>
        )}
      </RemoteShell>
    </section>
  );
}
