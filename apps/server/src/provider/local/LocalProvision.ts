import {
  LocalInferenceError,
  LocalInferenceInput,
  ProviderDriverKind,
  ProviderInstanceId,
} from "@t3tools/contracts";
import { resolveSelfInvocation, selfInvocationArgs } from "@t3tools/shared/nodeRuntime";
import * as HostProcess from "@t3tools/shared/HostProcess";
import * as Context from "effect/Context";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as Schema from "effect/Schema";
import * as Semaphore from "effect/Semaphore";
import * as ServerSettings from "../../serverSettings.ts";
import { listLocalModels, localOllamaEndpoint } from "./LocalAgent.ts";

const make = Effect.gen(function* () {
  const settings = yield* ServerSettings.ServerSettingsService;
  const env = yield* HostProcess.Environment;
  const platform = yield* HostProcess.Platform;
  const invocation = yield* resolveSelfInvocation();
  const gate = yield* Semaphore.make(1);
  const inventory = Effect.fn("LocalProvision.inventory")(function* (
    input: typeof LocalInferenceInput.Type,
  ) {
    if (platform !== "darwin")
      return yield* new LocalInferenceError({
        detail: "The local Heiwa worker currently requires macOS.",
      });
    const raw = input.endpoint ?? "http://127.0.0.1:11434";
    const endpoint = yield* Effect.try({
      try: () => localOllamaEndpoint(raw).origin,
      catch: () => new LocalInferenceError({ detail: "Choose an HTTP loopback Ollama endpoint." }),
    });
    const models = yield* Effect.tryPromise({
      try: (signal) => listLocalModels(endpoint, signal),
      catch: () =>
        new LocalInferenceError({
          detail:
            "Ollama is unavailable, or its installed models do not expose supported metadata. Start Ollama and retry.",
        }),
    });
    return { endpoint, models };
  });
  return {
    inventory,
    connect: Effect.fn("LocalProvision.connect")(
      function* (input: typeof LocalInferenceInput.Type) {
        const { endpoint, models } = yield* inventory(input);
        const model =
          input.model ?? models.find((value) => value.startsWith("gemma4:")) ?? models[0];
        if (!model || !models.includes(model))
          return yield* new LocalInferenceError({
            detail: "Install a local model with completion and tool support before connecting it.",
          });
        return yield* gate.withPermits(1)(
          Effect.gen(function* () {
            const instanceId = ProviderInstanceId.make("heiwa_ollama");
            const current = yield* settings.getSettings;
            const existing = current.providerInstances[instanceId];
            if (
              existing &&
              (existing.driver !== "acpRegistry" ||
                !Schema.is(Schema.Struct({ provision: Schema.Literal("heiwa-local") }))(
                  existing.config,
                ))
            )
              return yield* new LocalInferenceError({
                detail:
                  "The reserved local-provider id already belongs to another configuration. Rename that instance before connecting Ollama.",
              });
            yield* settings.updateProviderInstance({
              operation: existing ? "upsert" : "create",
              instanceId,
              instance: {
                driver: ProviderDriverKind.make("acpRegistry"),
                displayName: existing?.displayName ?? "Ollama · local",
                enabled: true,
                config: {
                  enabled: true,
                  provision: "heiwa-local",
                  source: "local",
                  commandPath: invocation.command,
                  commandArgs: selfInvocationArgs(invocation, ["__ollama-acp"]),
                },
                environment: [
                  { name: "ELECTRON_RUN_AS_NODE", value: "1", sensitive: false },
                  { name: "HEIWA_LOCAL_ENDPOINT", value: endpoint, sensitive: false },
                  { name: "HEIWA_LOCAL_MODEL", value: model, sensitive: false },
                  ...(env.HEIWA_APPLE_HELPER
                    ? [
                        {
                          name: "HEIWA_APPLE_HELPER",
                          value: env.HEIWA_APPLE_HELPER,
                          sensitive: false,
                        },
                      ]
                    : []),
                ],
              },
            });
            return { instanceId, endpoint, model, status: "configured" as const };
          }),
        );
      },
      (effect) =>
        effect.pipe(
          Effect.mapError((error) =>
            Schema.is(LocalInferenceError)(error)
              ? error
              : new LocalInferenceError({
                  detail: "Heiwa could not save the local inference provision.",
                }),
          ),
        ),
    ),
  };
});
export class LocalProvision extends Context.Service<LocalProvision, Effect.Success<typeof make>>()(
  "t3/provider/local/LocalProvision",
) {}
export const layer = Layer.effect(LocalProvision, make);
