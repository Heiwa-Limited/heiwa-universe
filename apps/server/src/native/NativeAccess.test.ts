import { assert, it } from "@effect/vitest";
import {
  EnvironmentId,
  MessageId,
  ProviderInstanceId,
  RunId,
  ScheduledTaskId,
  ThreadId,
} from "@t3tools/contracts";
import * as Effect from "effect/Effect";
import * as McpInvocationContext from "../mcp/McpInvocationContext.ts";
import {
  requireAttendedTurn,
  requireIntakeClient,
  requireOwnerClient,
} from "../mcp/toolkits/native/handlers.ts";
const base = {
  environmentId: EnvironmentId.make("test"),
  requestNamespace: "test",
  capabilities: new Set<McpInvocationContext.McpCapability>(),
  issuedAt: 0,
};
it.effect(
  "runtime agents and linked/read-only clients cannot approve or broaden Apple access",
  () =>
    Effect.gen(function* () {
      const scopes: McpInvocationContext.McpInvocationScope[] = [
        {
          ...base,
          client: undefined,
          thread: {
            threadId: ThreadId.make("thread"),
            providerSessionId: "session",
            providerInstanceId: ProviderInstanceId.make("codex"),
          },
        },
        {
          ...base,
          thread: undefined,
          client: { sessionId: "s", label: "read-only", access: "read-only" },
        },
        {
          ...base,
          thread: undefined,
          client: { sessionId: "s", label: "linked", access: "full-access", linked: true },
        },
      ];
      for (const scope of scopes)
        assert.equal(
          (yield* requireOwnerClient.pipe(
            Effect.provideService(McpInvocationContext.McpInvocationContext, scope),
            Effect.result,
          ))._tag,
          "Failure",
        );
      // Intake consumes mail: an agent's thread may run it, a read-only client may not.
      const [agent, readOnly] = scopes;
      yield* requireIntakeClient.pipe(
        Effect.provideService(McpInvocationContext.McpInvocationContext, agent!),
      );
      assert.equal(
        (yield* requireIntakeClient.pipe(
          Effect.provideService(McpInvocationContext.McpInvocationContext, readOnly!),
          Effect.result,
        ))._tag,
        "Failure",
      );
      yield* requireOwnerClient.pipe(
        Effect.provideService(McpInvocationContext.McpInvocationContext, {
          ...base,
          thread: undefined,
          client: { sessionId: "owner", label: "owner", access: "full-access" },
        }),
      );
    }),
);

it.effect("only a turn the user typed may apply Apple changes without review", () =>
  Effect.gen(function* () {
    const threadId = ThreadId.make("thread");
    const runId = RunId.make("run");
    const messageId = MessageId.make("message");
    const typed = { createdBy: "user", creationSource: "web" } as const;
    const attempt = (
      message: {
        readonly createdBy: string;
        readonly creationSource: string;
        readonly scheduledTaskId?: ScheduledTaskId;
      } | null,
      options: {
        readonly activeRunId?: RunId | null;
        readonly subagent?: boolean;
        readonly backgroundKind?: "subagent" | "command";
      } = {},
    ) =>
      requireAttendedTurn(
        {
          caller: {
            id: threadId,
            activeRunId: options.activeRunId === undefined ? runId : options.activeRunId,
            pendingBackgroundTasks:
              options.backgroundKind === undefined
                ? []
                : [{ taskId: "task", kind: options.backgroundKind }],
          },
          threads: {
            getThreadRecords: (_id, _fields, filter) =>
              Effect.succeed(
                (filter?.runIds === undefined
                  ? { messages: message === null ? [] : [{ id: messageId, ...message }] }
                  : { runs: [{ id: runId, userMessageId: messageId }] }) as never,
              ),
          },
        },
        options.subagent === true,
      ).pipe(
        Effect.result,
        Effect.map((result) => result._tag),
      );
    assert.equal(yield* attempt(typed), "Success");
    assert.equal(yield* attempt({ ...typed, creationSource: "mobile" }), "Success");
    assert.equal(yield* attempt(typed, { backgroundKind: "command" }), "Success");
    for (const unattended of [
      { ...typed, scheduledTaskId: ScheduledTaskId.make("task") },
      // A usage-limit resume: the server writes it and stamps it `user`.
      { createdBy: "user", creationSource: "server" },
      { createdBy: "agent", creationSource: "mcp" },
      { createdBy: "system", creationSource: "server" },
      null,
    ])
      assert.equal(yield* attempt(unattended), "Failure");
    assert.equal(yield* attempt(typed, { activeRunId: null }), "Failure");
    assert.equal(yield* attempt(typed, { subagent: true }), "Failure");
    assert.equal(yield* attempt(typed, { backgroundKind: "subagent" }), "Failure");
  }),
);
