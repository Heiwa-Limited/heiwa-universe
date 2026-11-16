import { WS_METHODS } from "@t3tools/contracts";
import {
  createEnvironmentRpcCommand,
  createEnvironmentRpcQueryAtomFamily,
  createEnvironmentRpcSubscriptionAtomFamily,
} from "@t3tools/client-runtime/state/runtime";
import { connectionAtomRuntime } from "../connection/runtime";
import { environmentAtomList } from "./environmentAtomList";
export const nativeStatus = createEnvironmentRpcQueryAtomFamily(connectionAtomRuntime, {
  label: "heiwa:native:status",
  tag: WS_METHODS.nativeStatus,
  staleTimeMs: 5_000,
  idleTtlMs: 60_000,
});
export const nativeStatusLive = createEnvironmentRpcSubscriptionAtomFamily(connectionAtomRuntime, {
  label: "heiwa:native:status:live",
  tag: WS_METHODS.nativeSubscribe,
});
export const nativeStatusListAtom = environmentAtomList(
  "heiwa:native:status:list",
  (environmentId) => nativeStatusLive({ environmentId, input: {} }),
);
export const nativeInventory = createEnvironmentRpcCommand(connectionAtomRuntime, {
  label: "heiwa:native:inventory",
  tag: WS_METHODS.nativeInventory,
});
export const nativeSelect = createEnvironmentRpcCommand(connectionAtomRuntime, {
  label: "heiwa:native:select",
  tag: WS_METHODS.nativeSelect,
});
export const nativeApprove = createEnvironmentRpcCommand(connectionAtomRuntime, {
  label: "heiwa:native:approve",
  tag: WS_METHODS.nativeApprove,
});

export const nativeResolve = createEnvironmentRpcCommand(connectionAtomRuntime, {
  label: "heiwa:native:resolve",
  tag: WS_METHODS.nativeResolve,
});
export const ensureScratchProject = createEnvironmentRpcCommand(connectionAtomRuntime, {
  label: "heiwa:native:scratch-project",
  tag: WS_METHODS.projectsEnsureScratch,
});
