import {
  LocalInferenceInput,
  LocalInferenceInventory,
  LocalInferenceConnection,
  LocalInferenceError,
  OrchestratorMcpFailure,
} from "@t3tools/contracts";
import * as Schema from "effect/Schema";
import { Tool, Toolkit } from "effect/ai";
import * as LocalProvision from "../../../provider/local/LocalProvision.ts";
import * as ThreadManagementService from "../../../orchestration-v2/ThreadManagementService.ts";
import * as McpInvocationContext from "../../McpInvocationContext.ts";
const dependencies = [
  LocalProvision.LocalProvision,
  ThreadManagementService.ThreadManagementService,
  McpInvocationContext.McpInvocationContext,
];
const failure = Schema.Union([LocalInferenceError, OrchestratorMcpFailure]);
export const LocalToolkit = Toolkit.make(
  Tool.make("heiwa_local_models", {
    description:
      "List installed loopback Ollama models that declare completion and tool support. No inference is performed; this does not prove successful work.",
    parameters: LocalInferenceInput,
    success: LocalInferenceInventory,
    failure,
    dependencies,
  })
    .annotate(Tool.Readonly, true)
    .annotate(Tool.OpenWorld, false),
  Tool.make("heiwa_local_connect", {
    description:
      "Owner setup: connect the Heiwa local worker to an installed Ollama model. Creates or updates its managed provider instance. Runtime agents cannot change provisions.",
    parameters: LocalInferenceInput,
    success: LocalInferenceConnection,
    failure,
    dependencies,
  })
    .annotate(Tool.Readonly, false)
    .annotate(Tool.OpenWorld, false),
);
