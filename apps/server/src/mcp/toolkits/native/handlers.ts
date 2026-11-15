import {
  NativeWorkflowError,
  type OrchestrationV2ConversationMessage,
  type OrchestrationV2ThreadShell,
} from "@t3tools/contracts";
import * as Effect from "effect/Effect";
import * as NativeWorkflows from "../../../native/NativeWorkflows.ts";
import type * as ThreadManagementService from "../../../orchestration-v2/ThreadManagementService.ts";
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
type TurnOrigin = Pick<
  OrchestrationV2ConversationMessage,
  "createdBy" | "creationSource" | "scheduledTaskId"
>;
/** The server also stamps `user` on messages it writes itself, such as a usage-limit resume. */
const typedByUser = (message: TurnOrigin) =>
  message.createdBy === "user" &&
  (message.creationSource === "web" || message.creationSource === "mobile") &&
  message.scheduledTaskId === undefined;

/**
 * Other turns read untrusted content with nobody watching, so they may only propose. The
 * credential belongs to the provider session, not the run: a subagent could outlive its run.
 */
export const requireAttendedTurn = Effect.fn("NativeToolkit.requireAttendedTurn")(function* (
  caller: {
    readonly caller:
      | Pick<OrchestrationV2ThreadShell, "id" | "activeRunId" | "pendingBackgroundTasks">
      | undefined;
    readonly threads: Pick<
      ThreadManagementService.ThreadManagementService["Service"],
      "getThreadRecords"
    >;
  },
  subagent: boolean,
) {
  const thread = caller.caller;
  const runId = thread?.activeRunId ?? null;
  if (
    subagent ||
    thread === undefined ||
    runId === null ||
    (thread.pendingBackgroundTasks ?? []).some((task) => task.kind === "subagent")
  )
    return yield* NativeWorkflows.reviewRequired();
  const read = Effect.gen(function* () {
    const { runs } = yield* caller.threads.getThreadRecords(thread.id, ["runs"], {
      runIds: [runId],
    });
    const run = runs[0];
    if (run === undefined) return undefined;
    const { messages } = yield* caller.threads.getThreadRecords(thread.id, ["messages"], {
      messageIds: [run.userMessageId],
    });
    return messages[0];
  });
  const message = yield* read.pipe(Effect.orElseSucceed(() => undefined));
  if (message === undefined || !typedByUser(message))
    return yield* NativeWorkflows.reviewRequired();
});
export const layer = McpToolAccess.toLayer(
  NativeToolkit,
  Effect.gen(function* () {
    const service = yield* NativeWorkflows.NativeWorkflows;
    return {
      heiwa_workflow: McpToolAccess.reads((input) =>
        Effect.gen(function* () {
          const scope = yield* McpInvocationContext.McpInvocationContext;
          // Intake hands each message out once, so a read-only client must not be able to drain it.
          if (input.workflow === "mail-intake" && scope.client?.access === "read-only")
            return yield* new NativeWorkflowError({
              code: "permission",
              detail: "Mail intake needs a client that may make changes.",
            });
          const result = yield* service.run(input);
          return { ...result, status: { ...result.status, pending: [] } };
        }),
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
            Effect.mapError(
              (error) => new NativeWorkflowError({ code: "permission", detail: error.message }),
            ),
            Effect.flatMap((caller) =>
              requireAttendedTurn(caller, scope.thread?.subagent === true),
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
