import { createSignal, For, onCleanup, onMount, Show, type JSX } from "solid-js";
import type { DiscoveredResource, OnboardingState } from "../state/types";
import "./first-run.css";

/** Desktop setup acknowledges a local workspace; resource detection grants no access. */
export function FirstRun(props: {
  state: OnboardingState;
  onEstablishIdentity: (displayName: string) => void | Promise<void>;
  onRecheck: () => void | Promise<void>;
  onVerifyProviders?: () => void | Promise<void>;
  onCompleteWorkspace?: () => void | Promise<void>;
  onOpenResourceGuide?: (id: string) => void | Promise<void>;
  onOpenSurface?: (surface: NonNullable<DiscoveredResource["surface"]>) => void | Promise<void>;
  onClose?: () => void;
  deviceDetails?: JSX.Element;
}) {
  const [name, setName] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string>();
  let panel!: HTMLElement;
  const identityGap = () => props.state.gaps.find((gap) => gap.step === "identity");
  const resources = (category: string) => props.state.workspace?.resources.filter((r) => r.category === category) ?? [];
  const detected = (resource: DiscoveredResource) => resource.app_detected || resource.tools_detected.length > 0 || resource.registered_accounts > 0;
  const run = async (action: () => void | Promise<void>) => {
    if (busy()) return;
    setBusy(true);
    setError(undefined);
    try { await action(); }
    catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)); }
    finally { setBusy(false); }
  };
  const submitName = (event: Event) => {
    event.preventDefault();
    if (name().trim()) void run(() => props.onEstablishIdentity(name().trim()));
  };
  onMount(() => {
    const previous = document.activeElement as HTMLElement | null;
    panel.querySelector<HTMLElement>("input, .first-run-header button, .first-run-actions button:not(:disabled)")?.focus();
    onCleanup(() => previous?.isConnected && previous.focus());
  });
  const keyboard = (event: KeyboardEvent) => {
    if (event.key === "Escape" && props.onClose && !busy()) {
      event.preventDefault();
      props.onClose();
    }
    if (event.key !== "Tab") return;
    const focusable = Array.from(panel.querySelectorAll<HTMLElement>("button:not(:disabled), input:not(:disabled), summary"))
      .filter((element) => {
        // A closed details still exposes its summary, but not its descendants.
        for (let parent = element.parentElement; parent && parent !== panel; parent = parent.parentElement) {
          if (parent instanceof HTMLDetailsElement && !parent.open && !parent.firstElementChild?.contains(element)) return false;
        }
        return true;
      });
    const index = focusable.indexOf(document.activeElement as HTMLElement);
    if (event.shiftKey && index <= 0) { event.preventDefault(); focusable.at(-1)?.focus(); }
    if (!event.shiftKey && index === focusable.length - 1) { event.preventDefault(); focusable[0]?.focus(); }
  };

  const card = (resource: DiscoveredResource) => (
    <details class="resource-card">
      <summary>
        <strong>{resource.name}</strong>
        <span class="resource-fact">{detected(resource) ? "Found on this Mac" : "Not detected"}</span>
      </summary>
      <p>{resource.detail}</p>
      <p class="resource-evidence">
        App: {resource.app_detected ? "found" : "not found in standard locations"}
        {resource.tools_detected.length ? ` · Tools: ${resource.tools_detected.join(", ")}` : ""}
        {resource.category === "inference" ? ` · ${resource.registered_accounts} registered accounts` : ""}
      </p>
      <Show when={resource.surface && props.onOpenSurface}>
        <button class="first-run-button secondary" disabled={busy() || !props.state.workspace?.can_enter}
          onClick={() => void run(() => props.onOpenSurface!(resource.surface!))}>
          Open {resource.name === "Apple Calendar" ? "Calendar setup" : "Mail"}
        </button>
      </Show>
      <Show when={resource.has_guide && props.onOpenResourceGuide}>
        <button class="first-run-button secondary" disabled={busy()}
          onClick={() => void run(() => props.onOpenResourceGuide!(resource.id))}>
          Open provider setup guide
        </button>
      </Show>
    </details>
  );

  return (
    <div class="first-run" role="dialog" aria-modal="true" aria-labelledby="first-run-title" onKeyDown={keyboard}>
      <section class="first-run-panel" ref={panel} aria-busy={busy()}>
        <header class="first-run-header">
          <div>
            <p class="first-run-eyebrow">YOUR MAC · YOUR WORKSPACE</p>
            <h1 id="first-run-title" class="first-run-title">{props.onClose ? "Your resources" : "Set up Heiwa"}</h1>
          </div>
          <Show when={props.onClose}>
            <button class="first-run-button secondary" aria-label="Close resources" disabled={busy()} onClick={() => props.onClose?.()}>Done</button>
          </Show>
        </header>
        <p class="first-run-lede">
          {props.state.display_name ? `${props.state.display_name}, this workspace is yours. ` : "Start with a local workspace. "}
          Connect resources when you need them. You can enter without an inference provider.
        </p>
        <Show when={identityGap()}>
          <form class="first-run-identity" onSubmit={submitName}>
            <label class="first-run-label" for="first-run-name">What should Heiwa call you?</label>
            <div class="first-run-row">
              <input id="first-run-name" class="first-run-input" type="text" value={name()}
                onInput={(event) => setName(event.currentTarget.value)} placeholder="Your name" maxLength={120} disabled={busy()} />
              <button class="first-run-button" type="submit" disabled={busy() || !name().trim()}>Continue</button>
            </div>
          </form>
        </Show>
        <Show when={props.state.workspace}>
          <div class="resource-section">
            <h2>Inference & creation</h2>
            <p class="quiet">Local installation metadata only. Detection does not verify sign-in, model access, or media tools.</p>
            <For each={resources("inference")}>{card}</For>
          </div>
          <div class="resource-section">
            <h2>Apple apps</h2>
            <p class="quiet">Your content stays in its source app until you connect a supported resource. Each app lists the access this build actually supports.</p>
            <For each={resources("apple").filter(detected)}>{card}</For>
            <details class="resource-other">
              <summary>Other Apple apps ({resources("apple").filter((r) => !detected(r)).length})</summary>
              <For each={resources("apple").filter((r) => !detected(r))}>{card}</For>
            </details>
          </div>
        </Show>
        <Show when={props.deviceDetails}>
          <details class="setup-diagnostics"><summary>This device & runtime</summary>{props.deviceDetails}</details>
        </Show>
        <Show when={props.state.gaps.length}>
          <details class="setup-diagnostics" open={!props.state.workspace}>
            <summary>Inference & setup status ({props.state.gaps.length})</summary>
            <ol class="first-run-gaps">
              <For each={props.state.gaps}>{(gap) => (
                <li class="first-run-gap" data-step={gap.step}>
                  <span class="first-run-step">{gap.step.replace("_", " ")}</span>
                  <pre class="first-run-detail">{gap.detail}</pre>
                  <p class="first-run-remedy">{gap.remedy}</p>
                </li>
              )}</For>
            </ol>
          </details>
        </Show>
        <Show when={error()}><p class="setup-inline-error" role="alert">{error()}</p></Show>
        <footer class="first-run-actions">
          <button class="first-run-button secondary" type="button" onClick={() => void run(props.onRecheck)} disabled={busy()}>Check again</button>
          <Show when={props.onVerifyProviders}>
            <button class="first-run-button secondary" type="button" onClick={() => void run(props.onVerifyProviders!)} disabled={busy()}>Verify providers</button>
          </Show>
          <Show when={props.onCompleteWorkspace && !props.state.workspace?.setup_complete}>
            <button class="first-run-button" type="button" onClick={() => void run(props.onCompleteWorkspace!)} disabled={busy() || !props.state.workspace?.can_enter}>Enter workspace</button>
          </Show>
        </footer>
        <Show when={props.onVerifyProviders}><p class="resource-evidence">Verify providers may contact provider endpoints and register locally discovered accounts. It does not generate content.</p></Show>
      </section>
    </div>
  );
}
