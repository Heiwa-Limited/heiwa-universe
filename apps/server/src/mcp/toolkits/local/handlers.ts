import * as Effect from "effect/Effect";
import * as LocalProvision from "../../../provider/local/LocalProvision.ts";
import * as McpToolAccess from "../../McpToolAccess.ts";
import { requireOwnerClient } from "../../OwnerClient.ts";
import { LocalToolkit } from "./tools.ts";
export const layer = McpToolAccess.toLayer(
  LocalToolkit,
  Effect.gen(function* () {
    const service = yield* LocalProvision.LocalProvision;
    return {
      heiwa_local_models: McpToolAccess.reads(service.inventory),
      heiwa_local_connect: McpToolAccess.writesEnvironment((input) =>
        requireOwnerClient("connect local inference").pipe(Effect.andThen(service.connect(input))),
      ),
    };
  }),
);
