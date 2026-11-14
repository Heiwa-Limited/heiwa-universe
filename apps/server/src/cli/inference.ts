import { LocalInferenceInventory, LocalInferenceConnection } from "@t3tools/contracts";
import * as Console from "effect/Console";
import * as Effect from "effect/Effect";
import * as Schema from "effect/Schema";
import { Command, Flag } from "effect/cli";
import { baseDirFlag } from "./config.ts";
import { withThreadTools } from "./thread.ts";
const encode = Schema.encodeSync(Schema.fromJsonString(Schema.Unknown));
const flags = {
  baseDir: baseDirFlag,
  endpoint: Flag.String("endpoint").pipe(Flag.withDefault("http://127.0.0.1:11434")),
  model: Flag.String("model").pipe(Flag.withDefault("")),
};
export const inferenceCommand = Command.make("inference").pipe(
  Command.withDescription("Connect inference provisions to Heiwa's agent tools."),
  Command.withSubcommands([
    Command.make("models", flags).pipe(
      Command.withHandler((input) =>
        withThreadTools(input, (tools) =>
          tools.call("heiwa_local_models", { endpoint: input.endpoint }, LocalInferenceInventory),
        ).pipe(
          Effect.flatMap((value) => Console.log(encode(value))),
          Effect.tapError((error) =>
            Console.error(
              error instanceof Error ? error.message : "Local inference request failed.",
            ),
          ),
        ),
      ),
    ),
    Command.make("connect", flags).pipe(
      Command.withHandler((input) =>
        withThreadTools(input, (tools) =>
          tools.call(
            "heiwa_local_connect",
            { endpoint: input.endpoint, model: input.model || undefined },
            LocalInferenceConnection,
          ),
        ).pipe(
          Effect.flatMap((value) => Console.log(encode(value))),
          Effect.tapError((error) =>
            Console.error(
              error instanceof Error ? error.message : "Local inference request failed.",
            ),
          ),
        ),
      ),
    ),
  ]),
);
