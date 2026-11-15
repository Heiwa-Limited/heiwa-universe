import type { ScheduledTask } from "@t3tools/contracts";
import * as Context from "effect/Context";
import * as Effect from "effect/Effect";

/**
 * Asked when a timer fires, before a thread starts. False skips the run and moves the schedule
 * on, so a task that polls for something costs no model call while there is nothing to do.
 */
export class ScheduledTaskRunGate extends Context.Reference<
  (task: ScheduledTask) => Effect.Effect<boolean>
>("t3/scheduledTasks/ScheduledTaskRunGate", {
  defaultValue: () => () => Effect.succeed(true),
}) {}
