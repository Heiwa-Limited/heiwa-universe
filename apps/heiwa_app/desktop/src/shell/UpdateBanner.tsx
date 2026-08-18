import { createSignal, Show } from "solid-js";
import type { UpdateOffer } from "../runtime";
import "./update-banner.css";

/**
 * A published update, offered rather than applied.
 *
 * The shell supervises a runtime and installing replaces its own binary, so
 * this asks before it acts: the user is told what version is waiting and
 * chooses when the window relaunches into it. Silent self-replacement under
 * an operator mid-task is the behavior this deliberately does not have.
 *
 * A failed install stays on screen with the reason. Falling back to nothing
 * would leave the user believing they are on a version they are not.
 */
export function UpdateBanner(props: {
  offer: UpdateOffer;
  onInstall: () => void | Promise<void>;
}) {
  const [busy, setBusy] = createSignal(false);
  const [failure, setFailure] = createSignal<string | undefined>();

  const install = async () => {
    if (busy()) return;
    setBusy(true);
    setFailure(undefined);
    try {
      await props.onInstall();
    } catch (error) {
      setFailure(error instanceof Error ? error.message : String(error));
    } finally {
      // Reached only if the relaunch did not happen — a successful install
      // replaces the process.
      setBusy(false);
    }
  };

  return (
    <aside class="update-banner" role="status" aria-label="Update available">
      <div class="update-text">
        <strong class="update-headline">Heiwa {props.offer.version} is available</strong>
        <span class="update-detail">
          You are running {props.offer.current_version}. Installing relaunches the window.
        </span>
        <Show when={failure()}>
          <span class="update-failure">Update failed: {failure()}</span>
        </Show>
      </div>
      <button class="update-action" type="button" disabled={busy()} onClick={install}>
        {busy() ? "Installing…" : "Install and relaunch"}
      </button>
    </aside>
  );
}
