import { createEffect, createSignal } from "solid-js";
import { render } from "solid-js/web";
import { App } from "./app";
import { createAppState } from "./state/app";
import { connectLegacyEvents } from "./state/legacy-events";
import { checkForUpdate, installUpdate, type UpdateOffer } from "./runtime";
import type { OnboardingState } from "./state/types";

const root = document.querySelector<HTMLDivElement>("#app");
if (!root) throw new Error("#app root element is missing");

const SELECTED_SESSION_KEY = "heiwa.desktop.selected_session_id";
function rememberedSessionId(): string | undefined {
  try { return localStorage.getItem(SELECTED_SESSION_KEY) || undefined; } catch { return undefined; }
}
const state = createAppState({ initialSelectedSessionId: rememberedSessionId() });
createEffect(() => {
  const selected = state.sessions.selectedId();
  try {
    if (selected) localStorage.setItem(SELECTED_SESSION_KEY, selected);
    else localStorage.removeItem(SELECTED_SESSION_KEY);
  } catch { /* preference storage is optional */ }
});
/** Retained so a future teardown path can close the legacy socket. */
let disposeLegacyEvents: (() => void) | undefined;

/**
 * First-run state, undefined until the runtime answers. Undefined renders the
 * shell rather than a blank window — the projection probes providers, and a
 * slow probe must not read as a broken application.
 */
const [onboarding, setOnboarding] = createSignal<OnboardingState | undefined>();
const [onboardingError, setOnboardingError] = createSignal<string>();

/**
 * A published release newer than this shell, once the check answers. The
 * shipped bundle can replace itself, but only on the user's word — this is
 * what the offer is made from.
 */
const [update, setUpdate] = createSignal<UpdateOffer | undefined>();

/** Ask the runtime what first run still needs. */
async function refreshOnboarding(verifyProviders = false): Promise<void> {
  try {
    const { invoke, isTauri } = await import("@tauri-apps/api/core");
    if (!isTauri()) return;
    setOnboarding(await invoke<OnboardingState>("onboarding_state", { verifyProviders }));
    setOnboardingError(undefined);
  } catch (error) {
    setOnboardingError(error instanceof Error ? error.message : String(error));
    throw error;
  }
}

async function completeWorkspace(): Promise<void> {
  const { invoke } = await import("@tauri-apps/api/core");
  setOnboarding(await invoke<OnboardingState>("complete_workspace_setup"));
}

async function openResourceGuide(resourceId: string): Promise<void> {
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("open_resource_guide", { resourceId });
}

async function establishIdentity(displayName: string): Promise<void> {
  const { invoke } = await import("@tauri-apps/api/core");
  setOnboarding(
    await invoke<OnboardingState>("establish_identity", { displayName }),
  );
}

render(
  () => (
    <App
      state={state}
      onboarding={onboarding()}
      onEstablishIdentity={establishIdentity}
      onRecheckOnboarding={refreshOnboarding}
      onVerifyProviders={() => refreshOnboarding(true)}
      onCompleteWorkspace={completeWorkspace}
      onOpenResourceGuide={openResourceGuide}
      onboardingError={onboardingError()}
      update={update()}
      onInstallUpdate={installUpdate}
    />
  ),
  root,
);

/**
 * Boot: paint immediately, then fill in. Onboarding first because it decides
 * whether the rest is reachable at all; health next because the rail status
 * and composer hint read it; the operator stream last because it replays
 * history and then subscribes.
 */
async function boot(): Promise<void> {
  await refreshOnboarding().catch(() => undefined);
  // The shell refreshes the active surface on arrival, including the first
  // one, so boot only has to cover what every surface reads.
  await state.runtime.loadHealth();
  await state.sessions.load();
  if (!state.sessions.selectedId() && !state.sessions.error()) {
    await state.sessions.createThread().catch(() => undefined);
  }
  disposeLegacyEvents = connectLegacyEvents(state);
  // Last, and not awaited by anything the window needs: the check reaches the
  // network, and a published update is never a reason to hold the first paint.
  setUpdate(await checkForUpdate());
}

void boot();

// Close the legacy socket on unload so a reload does not leave a reconnect
// loop running against the old page.
window.addEventListener("beforeunload", () => {
  state.operator.dispose();
  disposeLegacyEvents?.();
});
