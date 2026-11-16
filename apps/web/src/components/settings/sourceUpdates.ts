import { isSourcePreviewVersion } from "@t3tools/contracts";
import { APP_VERSION } from "../../branding";
import { stackedThreadToast, toastManager } from "../ui/toast";

/** The source updater for a managed preview; release builds keep the feed updater. */
export function managedSourceUpdates() {
  return isSourcePreviewVersion(APP_VERSION) ? (window.desktopBridge?.sourceUpdates ?? null) : null;
}

/** Opens About and checks the followed channel; About shows the result and any blocker. */
export async function openSourceUpdates(
  navigate: (options: { readonly to: "/settings/about" }) => unknown,
): Promise<void> {
  void navigate({ to: "/settings/about" });
  const bridge = managedSourceUpdates();
  if (!bridge) return;
  try {
    const state = await bridge.getState();
    if (!state.supported || state.operation?.status === "running") return;
    await bridge.run({ action: "check", channel: state.trackedChannel });
  } catch (error) {
    toastManager.add(
      stackedThreadToast({
        type: "error",
        title: "Could not check for updates",
        description: error instanceof Error ? error.message : "The update check could not start.",
      }),
    );
  }
}
