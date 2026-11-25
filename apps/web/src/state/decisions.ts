import { WS_METHODS } from "@t3tools/contracts";
import {
  createEnvironmentRpcCommand,
  createEnvironmentRpcQueryAtomFamily,
} from "@t3tools/client-runtime/state/runtime";

import { connectionAtomRuntime } from "../connection/runtime";
import { environmentAtomList } from "./environmentAtomList";

/** Owner-only questions from this Mac's decisions engine, as A2UI v1.0 surfaces. */
export const decisionsStatus = createEnvironmentRpcQueryAtomFamily(connectionAtomRuntime, {
  label: "heiwa:decisions:status",
  tag: WS_METHODS.decisionsStatus,
  staleTimeMs: 15_000,
  idleTtlMs: 120_000,
  // The engine creates decisions on its own schedule (collector every minute, 06:00 run).
  refreshIntervalMs: 60_000,
});

export const decisionsStatusListAtom = environmentAtomList(
  "heiwa:decisions:status:list",
  (environmentId) => decisionsStatus({ environmentId, input: {} }),
);

export const decisionsAnswer = createEnvironmentRpcCommand(connectionAtomRuntime, {
  label: "heiwa:decisions:answer",
  tag: WS_METHODS.decisionsAnswer,
});
