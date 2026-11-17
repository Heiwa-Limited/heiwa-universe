import {
  buildAgentRunEntries,
  buildRunningTerminalEntries,
  type AgentRunEntry,
  type AgentRunState,
  type RunningTerminalEntry,
} from "@t3tools/client-runtime/state/activity";
import { scopedThreadKey } from "@t3tools/client-runtime/environment";
import { useNavigate } from "@tanstack/react-router";
import { ChevronRightIcon, TerminalIcon } from "lucide-react";
import type { ReactNode } from "react";

import { revealThreadTerminal } from "../../lib/revealThreadTerminal";
import { tickTextContent } from "../../lib/tickTextContent";
import { cn } from "../../lib/utils";
import { useQuietThreadShells, useThreadShells } from "../../state/entities";
import { useAllKnownTerminalSessions } from "../../state/terminalSessions";
import { buildThreadRouteParams } from "../../threadRoutes";
import { AppRouteModal, AppRouteModalClose } from "../AppRouteModal";
import { formatCompactElapsedSeconds } from "../chat/AgentElapsed";
import { forgetTimelinePosition } from "../chat/timelineScrollAnchoring";
import { useProjectLookups } from "../CommandPaletteLookups";
import { SettingsGroup } from "../settings/SettingsGroup";
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "../ui/empty";
import { ScrollArea } from "../ui/scroll-area";
import { WorkspacePageContainer } from "../WorkspacePageContainer";

const RUN_STATE: Record<AgentRunState, { label: string; action: string; dotClass: string }> = {
  approval: { label: "Needs approval", action: "Open approval", dotClass: "bg-warning" },
  input: { label: "Asked a question", action: "Open question", dotClass: "bg-info" },
  starting: { label: "Starting", action: "Open thread", dotClass: "bg-muted-foreground/60" },
  running: { label: "Running", action: "Open output", dotClass: "bg-success" },
  waiting: {
    label: "Waiting on another thread",
    action: "Open thread",
    dotClass: "bg-muted-foreground/60",
  },
};

/** Live shells and agent runs; each row opens the exact shell or the run where it stands. */
export function ActivityPage() {
  const terminals = useRunningTerminals();
  const runs = useAgentRuns();
  const navigate = useNavigate();

  const openTerminal = async (entry: RunningTerminalEntry) => {
    await navigate({
      to: "/$environmentId/$threadId",
      params: buildThreadRouteParams(entry.threadRef),
    });
    // After the route change: this modal makes the workspace inert while it is open.
    revealThreadTerminal(entry.threadRef, entry.terminalId);
  };
  const openRun = (entry: AgentRunEntry) => {
    forgetTimelinePosition(scopedThreadKey(entry.threadRef));
    void navigate({
      to: "/$environmentId/$threadId",
      params: buildThreadRouteParams(entry.threadRef),
    });
  };

  return (
    <AppRouteModal label="Activity">
      <div className="flex min-h-0 min-w-0 flex-1 flex-col">
        <header className="flex min-h-12 shrink-0 items-center gap-1 border-b ps-5 pe-2">
          <h1 className="min-w-0 flex-1 truncate text-sm font-medium">Activity</h1>
          <AppRouteModalClose />
        </header>
        <ScrollArea className="min-h-0 flex-1">
          <WorkspacePageContainer>
            {runs.length === 0 && terminals.length === 0 ? (
              <Empty>
                <EmptyHeader>
                  <EmptyTitle>Nothing is running</EmptyTitle>
                  <EmptyDescription>
                    Agent runs and busy terminals appear here while they are live.
                  </EmptyDescription>
                </EmptyHeader>
              </Empty>
            ) : null}
            {runs.length > 0 ? (
              <section>
                <h2 className="mb-4 text-sm font-semibold">Agent runs</h2>
                <SettingsGroup>
                  {runs.map((entry) => (
                    <ActivityRow
                      key={scopedThreadKey(entry.threadRef)}
                      leading={
                        <span
                          aria-hidden
                          className={cn("size-2 rounded-full", RUN_STATE[entry.state].dotClass)}
                        />
                      }
                      title={entry.threadTitle}
                      details={[
                        RUN_STATE[entry.state].label,
                        entry.projectTitle,
                        entry.providerName,
                      ]}
                      elapsedSince={entry.startedAt}
                      action={RUN_STATE[entry.state].action}
                      onOpen={() => openRun(entry)}
                    />
                  ))}
                </SettingsGroup>
              </section>
            ) : null}
            {terminals.length > 0 ? (
              <section>
                <h2 className="mb-4 text-sm font-semibold">Terminals</h2>
                <SettingsGroup>
                  {terminals.map((entry) => (
                    <ActivityRow
                      key={`${scopedThreadKey(entry.threadRef)}:${entry.terminalId}`}
                      leading={<TerminalIcon className="size-4 text-muted-foreground" />}
                      title={entry.label}
                      details={[
                        entry.threadTitle,
                        entry.projectTitle,
                        entry.label === entry.terminalName ? null : entry.terminalName,
                      ]}
                      elapsedSince={null}
                      action="Open shell"
                      onOpen={() => void openTerminal(entry)}
                    />
                  ))}
                </SettingsGroup>
              </section>
            ) : null}
          </WorkspacePageContainer>
        </ScrollArea>
      </div>
    </AppRouteModal>
  );
}

function useRunningTerminals(): RunningTerminalEntry[] {
  const sessions = useAllKnownTerminalSessions();
  const threads = useThreadShells();
  const { projectByKey } = useProjectLookups();
  return buildRunningTerminalEntries({ sessions, threads, projectTitleByKey: projectByKey });
}

function useAgentRuns(): AgentRunEntry[] {
  // Held through streaming ticks: rows change only when a run's state does.
  const threads = useQuietThreadShells();
  const { projectByKey } = useProjectLookups();
  return buildAgentRunEntries({ threads, projectTitleByKey: projectByKey });
}

function ActivityRow(props: {
  readonly leading: ReactNode;
  readonly title: string;
  readonly details: ReadonlyArray<string | null>;
  readonly elapsedSince: string | null;
  readonly action: string;
  readonly onOpen: () => void;
}) {
  const startedAtMs = props.elapsedSince === null ? Number.NaN : Date.parse(props.elapsedSince);
  const details = props.details.filter(
    (detail): detail is string => detail !== null && detail.length > 0,
  );
  return (
    <button
      type="button"
      className="flex w-full items-center gap-3 p-4 text-start outline-none first:rounded-t-xl last:rounded-b-xl hover:bg-accent/50 focus-visible:bg-accent/50"
      onClick={props.onOpen}
    >
      <span className="flex size-4 shrink-0 items-center justify-center">{props.leading}</span>
      <span className="min-w-0 flex-1">
        <span className="block truncate text-sm font-medium">{props.title}</span>
        <span className="mt-1 block truncate text-xs text-muted-foreground">
          {details.join(" · ")}
          {Number.isFinite(startedAtMs) ? (
            <>
              {details.length > 0 ? " · " : null}
              <span
                className="tabular-nums"
                ref={tickTextContent(
                  (nowMs) => formatCompactElapsedSeconds((nowMs - startedAtMs) / 1000),
                  true,
                )}
              />
            </>
          ) : null}
        </span>
      </span>
      <span className="flex shrink-0 items-center gap-1 text-xs text-muted-foreground">
        {props.action}
        <ChevronRightIcon className="size-3.5" />
      </span>
    </button>
  );
}
