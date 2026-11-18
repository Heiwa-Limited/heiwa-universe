import {
  EnvironmentId,
  ProjectId,
  ProviderInstanceId,
  RunId,
  ThreadId,
  type TerminalSummary,
} from "@t3tools/contracts";
import { describe, expect, it } from "vite-plus/test";

import { buildAgentRunEntries, buildRunningTerminalEntries } from "./activity.ts";
import type { ThreadRuntimeSummary } from "./models.ts";
import { EMPTY_TERMINAL_SESSION_STATE, type KnownTerminalSession } from "./terminalSession.ts";

const environmentId = EnvironmentId.make("local");
const projectTitleByKey = new Map([["local:web", { title: "Heiwa" }]]);

describe("buildRunningTerminalEntries", () => {
  it("lists busy shells of known threads with their command, thread and project", () => {
    const session = (
      threadId: string,
      terminalId: string,
      hasRunningSubprocess: boolean,
      label: string,
    ): KnownTerminalSession => {
      const summary: TerminalSummary = {
        threadId,
        terminalId,
        cwd: "/repo",
        worktreePath: null,
        status: "running",
        pid: 10,
        exitCode: null,
        exitSignal: null,
        hasRunningSubprocess,
        label,
        updatedAt: "2026-11-16T00:00:00.000Z",
      };
      return {
        target: { environmentId, threadId: ThreadId.make(threadId), terminalId },
        state: { ...EMPTY_TERMINAL_SESSION_STATE, summary, hasRunningSubprocess },
      };
    };

    const entries = buildRunningTerminalEntries({
      sessions: [
        session("busy-thread", "term-1", true, "sleep 300"),
        session("busy-thread", "term-2", false, "Terminal 2"),
        session("idle-thread", "term-1", false, "Terminal 1"),
        session("unknown-thread", "term-1", true, "npm run dev"),
      ],
      threads: [
        { environmentId, id: "busy-thread", title: "Build", projectId: "web" },
        { environmentId, id: "idle-thread", title: "Docs", projectId: "web" },
      ],
      projectTitleByKey,
    });

    expect(entries).toEqual([
      {
        threadRef: { environmentId, threadId: "busy-thread" },
        terminalId: "term-1",
        label: "sleep 300",
        terminalName: "Terminal 1",
        threadTitle: "Build",
        projectTitle: "Heiwa",
      },
    ]);
  });
});

describe("buildAgentRunEntries", () => {
  const runtime = (
    status: ThreadRuntimeSummary["status"],
    activityStartedAt: string | null,
  ): ThreadRuntimeSummary => ({
    status,
    activeRunId: RunId.make("run"),
    activityStartedAt,
    providerInstanceId: ProviderInstanceId.make("codex"),
    providerName: "Codex",
    lastError: null,
    updatedAt: "2026-11-16T00:00:00.000Z",
  });
  const thread = (
    id: string,
    overrides: Partial<Parameters<typeof buildAgentRunEntries>[0]["threads"][number]> = {},
  ) => ({
    environmentId,
    id: ThreadId.make(id),
    title: id,
    projectId: ProjectId.make("web"),
    archivedAt: null,
    deletedAt: null,
    hasPendingApprovals: false,
    hasPendingUserInput: false,
    pendingSubagentRequest: null,
    blockedOnThreadWait: false,
    runtime: null,
    latestRun: null,
    ...overrides,
  });

  it("keeps only live runs, waiting-on-you first, then newest started", () => {
    const entries = buildAgentRunEntries({
      threads: [
        thread("older-run", { runtime: runtime("running", "2026-11-16T01:00:00.000Z") }),
        thread("newer-run", { runtime: runtime("running", "2026-11-16T02:00:00.000Z") }),
        thread("question", {
          hasPendingUserInput: true,
          runtime: runtime("running", "2026-11-16T00:30:00.000Z"),
        }),
        thread("approval", {
          hasPendingApprovals: true,
          runtime: runtime("waiting", "2026-11-16T00:10:00.000Z"),
        }),
        thread("connecting", { runtime: runtime("queued", null) }),
        thread("parent", {
          pendingSubagentRequest: "user_input",
          runtime: runtime("running", "2026-11-16T00:00:00.000Z"),
        }),
        thread("settled", { runtime: runtime("completed", null) }),
        thread("idle-session", { runtime: runtime("idle", null) }),
        thread("failed", { runtime: runtime("failed", null) }),
        thread("archived", {
          archivedAt: "2026-11-16T03:00:00.000Z",
          runtime: runtime("running", "2026-11-16T03:00:00.000Z"),
        }),
      ],
      projectTitleByKey,
    });

    expect(entries.map((entry) => [entry.threadRef.threadId, entry.state])).toEqual([
      ["approval", "approval"],
      ["question", "input"],
      ["newer-run", "running"],
      ["older-run", "running"],
      ["parent", "waiting"],
      ["connecting", "starting"],
    ]);
    expect(entries[0]).toMatchObject({
      threadRef: { environmentId, threadId: "approval" },
      projectTitle: "Heiwa",
      providerName: "Codex",
      startedAt: "2026-11-16T00:10:00.000Z",
    });
  });
});
