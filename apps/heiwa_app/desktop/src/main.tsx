import { createSignal } from "solid-js";
import { render } from "solid-js/web";
import { App } from "./app";
import { createAppState, OPERATOR_THREAD_ID } from "./state/app";
import { connectLegacyEvents } from "./state/legacy-events";
import { checkForUpdate, installUpdate, type UpdateOffer } from "./runtime";
import type { OnboardingState } from "./state/types";

const root = document.querySelector<HTMLDivElement>("#app");
if (!root) throw new Error("#app root element is missing");

const state = createAppState();
/** Retained so a future teardown path can close the legacy socket. */
let disposeLegacyEvents: (() => void) | undefined;

/**
 * First-run state, undefined until the runtime answers. Undefined renders the
 * shell rather than a blank window — the projection probes providers, and a
 * slow probe must not read as a broken application.
 */
const [onboarding, setOnboarding] = createSignal<OnboardingState | undefined>();

/**
 * A published release newer than this shell, once the check answers. The
 * shipped bundle can replace itself, but only on the user's word — this is
 * what the offer is made from.
 */
const [update, setUpdate] = createSignal<UpdateOffer | undefined>();

/** Ask the runtime what first run still needs. */
async function refreshOnboarding(): Promise<void> {
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    setOnboarding(await invoke<OnboardingState>("onboarding_state"));
  } catch {
    // Outside Tauri (a browser-only dev server) there is no runtime to ask.
    // Leaving this undefined keeps the shell usable instead of asserting a
    // first-run state the app has no evidence for.
    setOnboarding(undefined);
  }
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
  await refreshOnboarding();
  // The shell refreshes the active surface on arrival, including the first
  // one, so boot only has to cover what every surface reads.
  await state.runtime.loadHealth();
  await state.operator.start(OPERATOR_THREAD_ID).catch(() => undefined);
  disposeLegacyEvents = connectLegacyEvents(state);
  // Last, and not awaited by anything the window needs: the check reaches the
  // network, and a published update is never a reason to hold the first paint.
  setUpdate(await checkForUpdate());
}

void boot();

// Close the legacy socket on unload so a reload does not leave a reconnect
// loop running against the old page.
window.addEventListener("beforeunload", () => disposeLegacyEvents?.());
