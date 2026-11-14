import { NativeWorkflowError } from "@t3tools/contracts";
import * as Effect from "effect/Effect";
import * as NativeWorkflows from "../../../native/NativeWorkflows.ts";
import * as McpInvocationContext from "../../McpInvocationContext.ts";
import * as McpToolAccess from "../../McpToolAccess.ts";
import { NativeToolkit } from "./tools.ts";
export const requireOwnerClient = McpInvocationContext.McpInvocationContext.pipe(
  Effect.flatMap((scope) =>
    scope.thread !== undefined ||
    scope.client?.access !== "full-access" ||
    scope.client.linked === true
      ? Effect.fail(
          new NativeWorkflowError({
            code: "permission",
            detail:
              "Only the user can connect Apple apps, choose resources or approve native changes. Open Apple apps in Heiwa settings.",
          }),
        )
      : Effect.void,
  ),
);
export const layer = McpToolAccess.toLayer(
  NativeToolkit,
  Effect.gen(function* () {
    const service = yield* NativeWorkflows.NativeWorkflows;
    return {
      heiwa_workflow: McpToolAccess.reads((input) =>
        service
          .run(input)
          .pipe(Effect.map((result) => ({ ...result, status: { ...result.status, pending: [] } }))),
      ),
      heiwa_native_execute: McpToolAccess.writesEnvironment((input, check) =>
        Effect.gen(function* () {
          const scope = yield* McpInvocationContext.McpInvocationContext;
          if (scope.thread === undefined)
            return yield* new NativeWorkflowError({
              code: "permission",
              detail: "Automatic Apple changes require a live agent chat.",
            });
          const context = yield* Effect.context<Effect.Services<typeof check>>();
          const guard = check.pipe(
            Effect.asVoid,
            Effect.mapError(
              (error) => new NativeWorkflowError({ code: "permission", detail: error.message }),
            ),
            Effect.provide(context),
          );
          return yield* service.execute(input).pipe(
            Effect.provideService(NativeWorkflows.NativeExecutionAuthority, {
              origin: scope.requestNamespace,
              check: guard,
            }),
          );
        }),
      ),
      heiwa_native_resolve: McpToolAccess.writesEnvironment((input) =>
        requireOwnerClient.pipe(Effect.andThen(service.resolve(input.id, input.note))),
      ),
      heiwa_native_read: McpToolAccess.reads(service.read),
      heiwa_native_propose: McpToolAccess.writes((input) =>
        McpInvocationContext.McpInvocationContext.pipe(
          Effect.flatMap((scope) => service.propose(input, scope.requestNamespace)),
        ),
      ),
      heiwa_native_inventory: McpToolAccess.reads((input) =>
        requireOwnerClient.pipe(
          Effect.andThen(
            service
              .inventory(input.app, input.requestAccess)
              .pipe(Effect.map((resources) => ({ resources }))),
          ),
        ),
      ),
      heiwa_native_select: McpToolAccess.writesEnvironment((input) =>
        requireOwnerClient.pipe(
          Effect.andThen(service.select(input.app, input.resourceIds, input.writeMode)),
        ),
      ),
      heiwa_native_approve: McpToolAccess.writesEnvironment((input) =>
        requireOwnerClient.pipe(Effect.andThen(service.approve(input.id, input.approve))),
      ),
    };
  }),
);
