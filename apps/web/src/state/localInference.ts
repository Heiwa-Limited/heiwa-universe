import { WS_METHODS } from "@t3tools/contracts";
import { createEnvironmentRpcCommand } from "@t3tools/client-runtime/state/runtime";
import { connectionAtomRuntime } from "../connection/runtime";
export const connectLocalInference = createEnvironmentRpcCommand(connectionAtomRuntime, {
  label: "heiwa:local:connect",
  tag: WS_METHODS.localConnect,
});
