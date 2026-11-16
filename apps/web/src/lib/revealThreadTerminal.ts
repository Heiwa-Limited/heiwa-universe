import type { ScopedThreadRef } from "@t3tools/contracts";

import { selectThreadRightPanelState, useRightPanelStore } from "../rightPanelStore";
import { useTerminalUiStateStore } from "../terminalUiStateStore";
import { focusTerminalOnNavigation } from "./terminalFocus";

export function revealThreadTerminal(threadRef: ScopedThreadRef, terminalId: string) {
  const panels = useRightPanelStore.getState();
  const surface = selectThreadRightPanelState(panels.byThreadKey, threadRef).surfaces.find(
    (surface) => surface.kind === "terminal" && surface.terminalIds.includes(terminalId),
  );
  if (surface) {
    panels.activateTerminal(threadRef, surface.id, terminalId);
    panels.activateSurface(threadRef, surface.id);
    focusTerminalOnNavigation(threadRef, terminalId);
    return;
  }
  useTerminalUiStateStore.getState().ensureTerminal(threadRef, terminalId, {
    open: true,
    active: true,
  });
  focusTerminalOnNavigation(threadRef, terminalId);
}

let paletteReveal: { readonly threadRef: ScopedThreadRef; readonly terminalId: string } | null =
  null;

/** The open palette makes the workspace inert, so its reveal waits for the palette to close. */
export function queueTerminalRevealFromPalette(threadRef: ScopedThreadRef, terminalId: string) {
  paletteReveal = { threadRef, terminalId };
}

/** Runs at the palette's focus restoration milestone; true when it took focus over. */
export function restoreTerminalRevealFromPalette(): boolean {
  if (paletteReveal === null) return false;
  const { threadRef, terminalId } = paletteReveal;
  paletteReveal = null;
  revealThreadTerminal(threadRef, terminalId);
  return true;
}
