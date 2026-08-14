import { render } from "solid-js/web";
import { App } from "./app";
import { createAppState, OPERATOR_THREAD_ID } from "./state/app";
import { connectLegacyEvents } from "./state/legacy-events";
import { surfaceById } from "./surfaces/registry";

const root = document.querySelector<HTMLDivElement>("#app");
if (!root) throw new Error("#app root element is missing");

const state = createAppState();

render(() => <App state={state} />, root);

/**
 * Boot: paint immediately, then fill in. Health first because the rail status
 * and composer hint read it; the initial surface's own data follows; the
 * operator stream starts last because it replays history and then subscribes.
 */
async function boot(): Promise<void> {
  await state.runtime.loadHealth();
  await surfaceById(state.view()).refresh?.(state);
  await state.operator.start(OPERATOR_THREAD_ID).catch(() => undefined);
  connectLegacyEvents(state);
}

void boot();
