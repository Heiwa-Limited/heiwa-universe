import { createSignal, For, Show } from "solid-js";
import type { ProviderConnection } from "../state/types";

export type ProviderConnectionActions = {
  onConnectApiProvider?: (provider: string, apiKey: string) => Promise<void>;
  onVerifyApiProvider?: (accountId: string) => Promise<void>;
  onDisconnectApiProvider?: (accountId: string) => Promise<void>;
};

const providers = [
  { id: "openai", name: "OpenAI" },
  { id: "anthropic", name: "Anthropic" },
  { id: "google", name: "Google" },
  { id: "openrouter", name: "OpenRouter" },
];
const label = (provider: string) => providers.find((entry) => entry.id === provider)?.name ?? provider;
const statusLabel = (status: ProviderConnection["status"]) => ({
  connected: "Connected", disconnected: "Disconnected",
  needs_verification: "Needs verification", verification_failed: "Verification failed",
})[status];

/** Secrets live in this form only until submission, never in saved UI state. */
export function ProviderConnections(props: ProviderConnectionActions & { connections: ProviderConnection[] }) {
  const [provider, setProvider] = createSignal("openai");
  const [key, setKey] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string>();
  const [notice, setNotice] = createSignal<string>();
  const run = async (action: () => Promise<void>, success: string) => {
    if (busy()) return;
    setBusy(true); setError(undefined); setNotice(undefined);
    try { await action(); setNotice(success); }
    catch { setError("The connection change could not be completed. Refresh resources, check your credential or network, and try again."); }
    finally { setBusy(false); }
  };
  const connect = (event: SubmitEvent) => {
    event.preventDefault();
    if (busy() || !key().trim() || !props.onConnectApiProvider) return;
    const submitted = key();
    setKey("");
    void run(() => props.onConnectApiProvider!(provider(), submitted), "Connection saved. Its status below shows the verification result.");
  };
  return <section class="provider-connections" aria-label="AI connections" aria-busy={busy()}>
    <h2>Your AI connections</h2>
    <p class="quiet">Connect an API account here, or use an installed provider tool. API usage is billed separately from ChatGPT or Claude subscriptions.</p>
    <Show when={props.connections.length} fallback={<p>No accounts registered in this workspace yet.</p>}>
      <ul class="connection-list">
        <For each={props.connections}>{(connection) => <li class="connection-row">
          <div><strong>{label(connection.provider)}</strong><span>{connection.channel} · {statusLabel(connection.status)} · {connection.model_count} models</span></div>
          <Show when={connection.can_manage_key}>
            <div class="connection-actions">
              <Show when={props.onVerifyApiProvider}><button type="button" class="first-run-button secondary" disabled={busy()} aria-label={`Verify ${label(connection.provider)} connection`} onClick={() => void run(() => props.onVerifyApiProvider!(connection.account_id), "Connection checked. Review its status above.")}>Verify</button></Show>
              <Show when={props.onDisconnectApiProvider}><button type="button" class="first-run-button secondary" disabled={busy()} aria-label={`Disconnect ${label(connection.provider)} connection`} onClick={() => void run(() => props.onDisconnectApiProvider!(connection.account_id), "Connection removed. Existing conversations and artifacts are retained.")}>Disconnect</button></Show>
            </div>
          </Show>
        </li>}</For>
      </ul>
    </Show>
    <Show when={props.onConnectApiProvider}>
      <details class="connection-add">
        <summary>Add an API connection</summary>
        <form onSubmit={connect}>
          <label>Provider<select value={provider()} disabled={busy()} onChange={(event) => setProvider(event.currentTarget.value)}><For each={providers}>{(entry) => <option value={entry.id}>{entry.name}</option>}</For></select></label>
          <label>API key<input type="password" value={key()} maxLength={16_384} disabled={busy()} autocomplete="off" autocapitalize="off" spellcheck={false} onInput={(event) => setKey(event.currentTarget.value)} /></label>
          <p class="quiet">Saved in your operating system’s credential store. Verification checks model availability without generating content.</p>
          <button type="submit" class="first-run-button" disabled={busy() || !key().trim()}>{busy() ? "Connecting…" : "Connect account"}</button>
        </form>
      </details>
    </Show>
    <Show when={error()}><p class="setup-inline-error" role="alert">{error()}</p></Show>
    <Show when={notice()}><p role="status">{notice()}</p></Show>
  </section>;
}
