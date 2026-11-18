import type { EnvironmentId, ScopedThreadRef } from "@t3tools/contracts";
import { getTerminalLabel, resolveTerminalSessionLabel } from "@t3tools/shared/terminalLabels";

import { scopeThreadRef } from "../environment/scoped.ts";
import { resolveThreadWorkingStartedAt, type EnvironmentThreadShell } from "./models.ts";
import type { KnownTerminalSession } from "./terminalSession.ts";

type ProjectTitleByKey = ReadonlyMap<string, { readonly title: string }>;

export interface RunningTerminalEntry {
  readonly threadRef: ScopedThreadRef;
  readonly terminalId: string;
  /** The running command when the server knows it, else the terminal's name. */
  readonly label: string;
  readonly terminalName: string;
  readonly threadTitle: string;
  readonly projectTitle: string | null;
}

/** Busy shells whose thread this client knows, so each one has a thread to open. */
export function buildRunningTerminalEntries(input: {
  readonly sessions: ReadonlyArray<KnownTerminalSession>;
  readonly threads: ReadonlyArray<{
    readonly environmentId: EnvironmentId;
    readonly id: string;
    readonly title: string;
    readonly projectId: string;
  }>;
  readonly projectTitleByKey: ProjectTitleByKey;
}): RunningTerminalEntry[] {
  const threadByKey = new Map(
    input.threads.map((thread) => [`${thread.environmentId}:${thread.id}`, thread]),
  );
  return input.sessions.flatMap((session) => {
    if (!session.state.hasRunningSubprocess) return [];
    const { environmentId, threadId, terminalId } = session.target;
    const thread = threadByKey.get(`${environmentId}:${threadId}`);
    if (!thread) return [];
    return [
      {
        threadRef: scopeThreadRef(environmentId, threadId),
        terminalId,
        label: resolveTerminalSessionLabel(terminalId, session.state.summary),
        terminalName: getTerminalLabel(terminalId),
        threadTitle: thread.title,
        projectTitle:
          input.projectTitleByKey.get(`${environmentId}:${thread.projectId}`)?.title ?? null,
      },
    ];
  });
}

/** Waiting on the user first: an open approval, then an open question. */
export type AgentRunState = "approval" | "input" | "starting" | "running" | "waiting";

type AgentRunThread = Pick<
  EnvironmentThreadShell,
  | "environmentId"
  | "id"
  | "title"
  | "projectId"
  | "archivedAt"
  | "deletedAt"
  | "hasPendingApprovals"
  | "hasPendingUserInput"
  | "pendingSubagentRequest"
  | "blockedOnThreadWait"
  | "runtime"
  | "latestRun"
>;

/**
 * A thread's own run, or null when nothing is live there. A subagent's request
 * counts on the subagent's thread, which has the prompt; its parent reads as waiting.
 */
function resolveAgentRunState(thread: AgentRunThread): AgentRunState | null {
  if (thread.archivedAt !== null || thread.deletedAt !== null) return null;
  if (thread.hasPendingApprovals) return "approval";
  if (thread.hasPendingUserInput) return "input";
  switch (thread.runtime?.status) {
    case "preparing":
    case "queued":
    case "starting":
      return "starting";
    case "running":
    case "waiting":
      return thread.blockedOnThreadWait === true || thread.pendingSubagentRequest != null
        ? "waiting"
        : "running";
    default:
      return null;
  }
}

export interface AgentRunEntry {
  readonly threadRef: ScopedThreadRef;
  readonly state: AgentRunState;
  readonly threadTitle: string;
  readonly projectTitle: string | null;
  readonly providerName: string | null;
  readonly startedAt: string | null;
}

const STATE_RANK: Record<AgentRunState, number> = {
  approval: 0,
  input: 1,
  starting: 2,
  running: 2,
  waiting: 2,
};

/** Live runs, the ones waiting on the user first, then the most recently started. */
export function buildAgentRunEntries(input: {
  readonly threads: ReadonlyArray<AgentRunThread>;
  readonly projectTitleByKey: ProjectTitleByKey;
}): AgentRunEntry[] {
  const entries = input.threads.flatMap((thread): AgentRunEntry[] => {
    const state = resolveAgentRunState(thread);
    if (state === null) return [];
    return [
      {
        threadRef: scopeThreadRef(thread.environmentId, thread.id),
        state,
        threadTitle: thread.title,
        projectTitle:
          input.projectTitleByKey.get(`${thread.environmentId}:${thread.projectId}`)?.title ?? null,
        providerName: thread.runtime?.providerName ?? null,
        startedAt: resolveThreadWorkingStartedAt(thread),
      },
    ];
  });
  const startedAtMs = (entry: AgentRunEntry) =>
    entry.startedAt === null ? 0 : Date.parse(entry.startedAt);
  return entries.sort(
    (left, right) =>
      STATE_RANK[left.state] - STATE_RANK[right.state] ||
      startedAtMs(right) - startedAtMs(left) ||
      left.threadRef.threadId.localeCompare(right.threadRef.threadId),
  );
}
