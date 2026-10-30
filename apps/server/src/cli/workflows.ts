import {
  NativeAction,
  NativeApp,
  NativeProposal,
  NativeReadResult,
  NativeResource,
  NativeStatus,
  NativeWorkflowResult,
  NativeWorkflowError,
} from "@t3tools/contracts";
import * as Console from "effect/Console";
import * as Effect from "effect/Effect";
import * as Schema from "effect/Schema";
import { Argument, Command, Flag } from "effect/cli";
import { baseDirFlag } from "./config.ts";
import { withThreadTools } from "./thread.ts";
const withNativeTools: typeof withThreadTools = (flags, run) =>
  withThreadTools(flags, run).pipe(
    Effect.tapError((error) =>
      Console.error(error instanceof Error ? error.message : "The workflow request failed."),
    ),
  );

const encode = Schema.encodeSync(Schema.fromJsonString(Schema.Unknown));
const print = (value: unknown) => Console.log(encode(value));
const app = Argument.Literals("app", NativeApp.literals);
const ids = Flag.String("resources").pipe(
  Flag.withDefault(""),
  Flag.withDescription("Comma-separated exact ids. Empty revokes this app's selection."),
);
const range = {
  start: Flag.String("start").pipe(Flag.withDefault("")),
  end: Flag.String("end").pipe(Flag.withDefault("")),
  limit: Flag.Int("limit").pipe(Flag.withDefault(10)),
};
const catalog = [
  {
    id: "daily-brief",
    description: "Read the selected Apple schedule and reminder lists in one bounded response.",
    modelCalls: 0,
  },
  {
    id: "capture",
    description:
      "Stage a note and follow-up reminder together from one command. Requires selected Notes and Reminders resources; review in Apple apps.",
    modelCalls: 0,
  },
  {
    id: "native-audit",
    description:
      "Inspect selected Apple resources without requesting access or reading item contents.",
    modelCalls: 0,
  },
];
export const workflowsCommand = Command.make("workflows").pipe(
  Command.withDescription("Reusable workflows shared with Heiwa agents; compact JSON output."),
  Command.withHandler(() => print(catalog)),
  Command.withSubcommands([
    Command.make("capture", {
      title: Argument.String("title"),
      text: Flag.String("text"),
      notesFolder: Flag.String("notes-folder").pipe(Flag.withDefault("")),
      reminderList: Flag.String("reminder-list").pipe(Flag.withDefault("")),
      baseDir: baseDirFlag,
    }).pipe(
      Command.withDescription(
        "Stage a note and reminder; empty resource flags use the single selected folder/list. Nothing is applied until reviewed.",
      ),
      Command.withHandler((flags) =>
        withNativeTools(flags, (tools) =>
          Effect.gen(function* () {
            const audit = yield* tools.call(
              "heiwa_workflow",
              { workflow: "native-audit" },
              NativeWorkflowResult,
            );
            const resource = (app: "notes" | "reminders", requested: string) => {
              const ids = audit.status.grants.find((row) => row.app === app)?.resourceIds ?? [];
              const id = requested || (ids.length === 1 ? ids[0] : undefined);
              return id && ids.includes(id) ? id : undefined;
            };
            const folder = resource("notes", flags.notesFolder),
              list = resource("reminders", flags.reminderList);
            if (!folder || !list)
              return yield* new NativeWorkflowError({
                code: "selection",
                detail:
                  "Select one Notes folder and one Reminders list, or provide exact selected ids with --notes-folder and --reminder-list.",
              });
            const note = yield* tools.call(
              "heiwa_native_propose",
              {
                app: "notes",
                operation: "create",
                resourceId: folder,
                title: flags.title,
                body: flags.text,
              },
              NativeProposal,
            );
            const reminder = yield* tools
              .call(
                "heiwa_native_propose",
                {
                  app: "reminders",
                  operation: "create",
                  resourceId: list,
                  title: flags.title,
                  body: flags.text,
                },
                NativeProposal,
              )
              .pipe(
                Effect.tapError(() =>
                  print({
                    workflow: "capture",
                    applied: false,
                    proposals: [note],
                    detail:
                      "The note was staged; the reminder could not be staged. Review or reject the pending note in Apple apps.",
                  }),
                ),
              );
            yield* print({ workflow: "capture", applied: false, proposals: [note, reminder] });
          }),
        ),
      ),
    ),
    Command.make("list").pipe(Command.withHandler(() => print(catalog))),
    Command.make("run", {
      workflow: Argument.Literals("workflow", ["daily-brief", "native-audit"]),
      baseDir: baseDirFlag,
      ...range,
    }).pipe(
      Command.withHandler((flags) =>
        withNativeTools(flags, (tools) =>
          tools
            .call(
              "heiwa_workflow",
              {
                workflow: flags.workflow,
                start: flags.start || undefined,
                end: flags.end || undefined,
                limit: flags.limit,
              },
              NativeWorkflowResult,
            )
            .pipe(Effect.flatMap(print)),
        ),
      ),
    ),
  ]),
);
export const nativeCommand = Command.make("native").pipe(
  Command.withDescription(
    "Connect selected Apple resources, read, stage changes and approve exact proposals.",
  ),
  Command.withSubcommands([
    Command.make("resolve", {
      id: Argument.String("proposal-id"),
      note: Flag.String("note"),
      baseDir: baseDirFlag,
    }).pipe(
      Command.withHandler((flags) =>
        withNativeTools(flags, (tools) =>
          tools
            .call("heiwa_native_resolve", { id: flags.id, note: flags.note }, NativeProposal)
            .pipe(Effect.flatMap(print)),
        ),
      ),
    ),
    Command.make("inventory", {
      app,
      requestAccess: Flag.Boolean("request-access").pipe(Flag.withDefault(false)),
      baseDir: baseDirFlag,
    }).pipe(
      Command.withHandler((flags) =>
        withNativeTools(flags, (tools) =>
          tools
            .call(
              "heiwa_native_inventory",
              { app: flags.app, requestAccess: flags.requestAccess },
              Schema.Struct({ resources: Schema.Array(NativeResource) }),
            )
            .pipe(Effect.flatMap(print)),
        ),
      ),
    ),
    Command.make("select", {
      app,
      resources: ids,
      mode: Flag.Literals("mode", ["review", "automatic"]).pipe(Flag.withDefault("review")),
      baseDir: baseDirFlag,
    }).pipe(
      Command.withHandler((flags) =>
        withNativeTools(flags, (tools) =>
          tools
            .call(
              "heiwa_native_select",
              {
                app: flags.app,
                resourceIds: flags.resources ? flags.resources.split(",") : [],
                writeMode: flags.mode,
              },
              NativeStatus,
            )
            .pipe(Effect.flatMap(print)),
        ),
      ),
    ),
    Command.make("read", {
      app,
      includeCompleted: Flag.Boolean("include-completed").pipe(Flag.withDefault(false)),
      baseDir: baseDirFlag,
      ...range,
    }).pipe(
      Command.withHandler((flags) =>
        withNativeTools(flags, (tools) =>
          tools
            .call(
              "heiwa_native_read",
              {
                app: flags.app,
                includeCompleted: flags.includeCompleted,
                start: flags.start || undefined,
                end: flags.end || undefined,
                limit: flags.limit,
              },
              NativeReadResult,
            )
            .pipe(Effect.flatMap(print)),
        ),
      ),
    ),
    Command.make("propose", {
      action: Argument.String("action-json").pipe(
        Argument.withSchema(Schema.fromJsonString(NativeAction)),
      ),
      baseDir: baseDirFlag,
    }).pipe(
      Command.withHandler((flags) =>
        withNativeTools(flags, (tools) =>
          tools
            .call("heiwa_native_propose", flags.action, NativeProposal)
            .pipe(Effect.flatMap(print)),
        ),
      ),
    ),
    Command.make("approve", {
      id: Argument.String("proposal-id"),
      reject: Flag.Boolean("reject").pipe(Flag.withDefault(false)),
      baseDir: baseDirFlag,
    }).pipe(
      Command.withDescription(
        "Apply one exact reviewed proposal, or reject it. Uncertain writes never repeat automatically.",
      ),
      Command.withHandler((flags) =>
        withNativeTools(flags, (tools) =>
          tools
            .call("heiwa_native_approve", { id: flags.id, approve: !flags.reject }, NativeProposal)
            .pipe(Effect.flatMap(print)),
        ),
      ),
    ),
  ]),
);
