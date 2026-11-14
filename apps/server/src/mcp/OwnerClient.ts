import { OrchestratorMcpFailure } from "@t3tools/contracts";
import * as Effect from "effect/Effect";
import * as McpInvocationContext from "./McpInvocationContext.ts";
export const requireOwnerClient = (action: string) =>
  McpInvocationContext.McpInvocationContext.pipe(
    Effect.flatMap((scope) =>
      scope.thread !== undefined ||
      scope.client?.access !== "full-access" ||
      scope.client.linked === true
        ? Effect.fail(
            new OrchestratorMcpFailure({
              code: "capability_denied",
              message: `Only the user can ${action}. Open Heiwa settings.`,
            }),
          )
        : Effect.void,
    ),
  );
