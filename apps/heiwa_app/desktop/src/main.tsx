import { render } from "solid-js/web";
import { App } from "./app";
import { createAppState, OPERATOR_THREAD_ID } from "./state/app";
import { connectLegacyEvents } from "./state/legacy-events";

const root = document.querySelector<HTMLDivElement>("#app");
if (!root) throw new Error("#app root element is missing");

const state = createAppState();
/** Retained so a future teardown path can close the legacy socket. */
let disposeLegacyEvents: (() => void) | undefined;

render(() => <App state={state} />, root);

/**
 * Boot: paint immediately, then fill in. Health first because the rail status
 * and composer hint read it; the initial surface's own data follows; the
 * operator stream starts last because it replays history and then subscribes.
 */
async function boot(): Promise<void> {
  // The shell refreshes the active surface on arrival, including the first
  // one, so boot only has to cover what every surface reads.
  await state.runtime.loadHealth();
  await state.operator.start(OPERATOR_THREAD_ID).catch(() => undefined);
  disposeLegacyEvents = connectLegacyEvents(state);
}

void boot();

// Close the legacy socket on unload so a reload does not leave a reconnect
// loop running against the old page.
window.addEventListener("beforeunload", () => disposeLegacyEvents?.());
