import type { EnvironmentId } from "@t3tools/contracts";
import { useNavigate } from "@tanstack/react-router";
import { TerminalIcon } from "lucide-react";

import { queueTerminalRevealFromPalette } from "../lib/revealThreadTerminal";
import { useThreadShells } from "../state/entities";
import { useKnownTerminalSessions } from "../state/terminalSessions";
import { buildThreadRouteParams } from "../threadRoutes";
import {
  ADDON_ICON_CLASS,
  buildRunningTerminalEntries,
  ITEM_ICON_CLASS,
  type CommandPaletteActionItem,
  type CommandPaletteSubmenuItem,
} from "./CommandPalette.logic";
import { useProjectLookups } from "./CommandPaletteLookups";

export const RUNNING_TERMINALS_GROUP = "running-terminals";

/**
 * Busy shells in one environment, from the same permission-gated metadata the
 * sidebar's terminal indicator reads. Null when that metadata is unreadable.
 */
export function useCommandPaletteRunningTerminalsItem(
  environmentId: EnvironmentId | null,
): CommandPaletteSubmenuItem | null {
  const navigate = useNavigate();
  const sessions = useKnownTerminalSessions({ environmentId, threadId: null });
  const threads = useThreadShells();
  const { projectByKey } = useProjectLookups();
  if (sessions === null) return null;

  const items = buildRunningTerminalEntries({
    sessions,
    threads,
    projectTitleByKey: projectByKey,
  }).map((entry): CommandPaletteActionItem => ({
    kind: "action",
    value: `running-terminal:${entry.threadRef.environmentId}:${entry.threadRef.threadId}:${entry.terminalId}`,
    searchTerms: [entry.label, entry.terminalName, entry.threadTitle, entry.projectTitle ?? ""],
    title: entry.label,
    description: [
      entry.threadTitle,
      entry.projectTitle,
      entry.label === entry.terminalName ? null : entry.terminalName,
    ]
      .filter((part) => part !== null && part.length > 0)
      .join(" · "),
    icon: <TerminalIcon className={ITEM_ICON_CLASS} />,
    run: async () => {
      queueTerminalRevealFromPalette(entry.threadRef, entry.terminalId);
      await navigate({
        to: "/$environmentId/$threadId",
        params: buildThreadRouteParams(entry.threadRef),
      });
    },
  }));

  return {
    kind: "submenu",
    value: "action:running-terminals",
    searchTerms: ["running terminals", "terminal", "shell", "process", "busy", "activity"],
    title: "Running terminals",
    description: items.length === 0 ? "None running" : `${items.length} running`,
    disabled: items.length === 0,
    icon: <TerminalIcon className={ITEM_ICON_CLASS} />,
    addonIcon: <TerminalIcon className={ADDON_ICON_CLASS} />,
    groups:
      items.length === 0
        ? []
        : [{ value: RUNNING_TERMINALS_GROUP, label: "Running terminals", items }],
  };
}
