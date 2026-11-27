import * as Effect from "effect/Effect";
import * as LifeSignals from "../../../life/LifeSignals.ts";
import * as McpToolAccess from "../../McpToolAccess.ts";
import { LifeToolkit } from "./tools.ts";
export const layer = McpToolAccess.toLayer(
  LifeToolkit,
  Effect.gen(function* () {
    const service = yield* LifeSignals.LifeSignals;
    return { heiwa_life: McpToolAccess.reads(service.read) };
  }),
);
