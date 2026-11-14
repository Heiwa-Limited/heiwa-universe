import { assert, it } from "@effect/vitest";
import { EnvironmentId, ProviderInstanceId, ThreadId } from "@t3tools/contracts";
import * as Effect from "effect/Effect";
import * as McpInvocationContext from "../mcp/McpInvocationContext.ts";
import { requireOwnerClient } from "../mcp/toolkits/native/handlers.ts";
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
