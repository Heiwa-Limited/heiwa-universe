import { useAtomValue } from "@effect/atom-react";
import { AuthSettingsWriteScope, type NativeProposal } from "@t3tools/contracts";
import { AsyncResult } from "effect/reactivity";

import { environmentIdsKey } from "../../state/environmentAtomList";
import { useEnvironments } from "../../state/environments";
import { useThreadShells } from "../../state/entities";
import { nativeStatusListAtom } from "../../state/native";
import { useEnvironmentsWithScope } from "../../state/session";
import { resolveSidebarThreadStatus } from "../Sidebar.logic";

const WAITING_THREAD_STATUSES = new Set(["approval", "input", "request"]);

/** Everything waiting on the user: Apple changes on each Mac, and threads that asked for something. */
export function useApprovals() {
  const { environments } = useEnvironments();
  const owned = useEnvironmentsWithScope(environments, AuthSettingsWriteScope);
  // Apple changes are approved by the owner, so other sessions never open the stream.
  const sources = environments.filter(
    (environment) =>
      environment.connection.phase === "connected" &&
      environment.serverConfig !== null &&
      owned.has(environment.environmentId),
  );
  const results = useAtomValue(
    nativeStatusListAtom(environmentIdsKey(sources.map((source) => source.environmentId))),
  );
  const native = sources.map((source, index) => {
    const status = AsyncResult.getOrElse(results[index]!, () => null);
    const pending: ReadonlyArray<NativeProposal> = status?.pending ?? [];
    return {
      environmentId: source.environmentId,
      label: source.label,
      pending,
      recent: (status?.recent ?? []).filter(
        (item) => !pending.some((waiting) => waiting.id === item.id),
      ),
    };
  });
  const threads = useThreadShells().flatMap((thread) => {
    if (thread.archivedAt !== null) return [];
    const status = resolveSidebarThreadStatus(thread);
    return WAITING_THREAD_STATUSES.has(status) ? [{ thread, status }] : [];
  });
  return {
    native,
    threads,
    count: threads.length + native.reduce((total, source) => total + source.pending.length, 0),
  };
}
