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
import { requireAttendedTurn, requireOwnerClient } from "../mcp/toolkits/native/handlers.ts";
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
      yield* requireOwnerClient.pipe(
        Effect.provideService(McpInvocationContext.McpInvocationContext, {
          ...base,
          thread: undefined,
          client: { sessionId: "owner", label: "owner", access: "full-access" },
        }),
      );
    }),
);

it.effect("only a turn the user started may apply Apple changes without review", () =>
  Effect.gen(function* () {
    const threadId = ThreadId.make("thread");
    const runId = RunId.make("run");
    const messageId = MessageId.make("message");
    const attempt = (
      message: { readonly createdBy: string; readonly scheduledTaskId?: ScheduledTaskId } | null,
      activeRunId: RunId | null = runId,
    ) =>
      requireAttendedTurn({
        caller: { id: threadId, activeRunId },
        threads: {
          getThreadRecords: (_id, _fields, filter) =>
            Effect.succeed(
              (filter?.runIds === undefined
                ? { messages: message === null ? [] : [{ id: messageId, ...message }] }
                : { runs: [{ id: runId, userMessageId: messageId }] }) as never,
            ),
        },
      }).pipe(
        Effect.result,
        Effect.map((result) => result._tag),
      );
    assert.equal(yield* attempt({ createdBy: "user" }), "Success");
    for (const unattended of [
      { createdBy: "user", scheduledTaskId: ScheduledTaskId.make("task") },
      { createdBy: "agent" },
      { createdBy: "system" },
      null,
    ])
      assert.equal(yield* attempt(unattended), "Failure");
    assert.equal(yield* attempt({ createdBy: "user" }, null), "Failure");
  }),
);
