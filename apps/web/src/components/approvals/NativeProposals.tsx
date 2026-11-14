import { useState } from "react";
import type { EnvironmentId, NativeProposal } from "@t3tools/contracts";
import type { AtomCommandResult } from "@t3tools/client-runtime/state/runtime";
import * as Cause from "effect/Cause";

import { usePrimarySettings } from "../../hooks/useSettings";
import { nativeApprove, nativeResolve } from "../../state/native";
import { resolveHour12 } from "../../timestampFormat";
import { useAtomCommand } from "../../state/use-atom-command";
import { SettingsGroup } from "../settings/SettingsGroup";
import { Button } from "../ui/button";
import { groupBySource, proposalHeading, proposalWhen } from "./approvals.logic";

function ProposalContent({ proposal }: { proposal: NativeProposal }) {
  const action = proposal.action;
  const target = proposal.resource;
  const hour12 = usePrimarySettings((settings) => resolveHour12(settings.timestampFormat));
  const when = proposalWhen(action, { hour12 });
  return (
    <div className="min-w-0 space-y-2 text-sm">
      <p className="font-medium">{proposalHeading(action)}</p>
      {proposal.before ? (
        <details className="text-muted-foreground">
          <summary>Current item: {proposal.before.title}</summary>
          <p className="max-h-32 overflow-y-auto whitespace-pre-wrap">{proposal.before.body}</p>
        </details>
      ) : null}
      {action.title ? <p className="break-words">{action.title}</p> : null}
      {action.body ? (
        <p className="max-h-48 overflow-y-auto whitespace-pre-wrap break-words text-muted-foreground">
          {action.body}
        </p>
      ) : null}
      {action.due !== undefined ? (
        <p className="text-muted-foreground">
          {action.due === null ? "Remove due date" : `Due: ${action.due}`}
        </p>
      ) : null}
      {action.priority !== undefined ? (
        <p className="text-muted-foreground">
          Priority: {{ 0: "None", 1: "High", 5: "Medium", 9: "Low" }[action.priority]}
        </p>
      ) : null}
      {action.allDay !== undefined ? (
        <p>{action.allDay ? "All-day event" : "Timed event"}</p>
      ) : null}
      {when ? <p className="text-muted-foreground">{when}</p> : null}
      {action.app === "notes" && action.operation === "update" && action.body !== undefined ? (
        <p className="text-muted-foreground">
          Replaces the note body with plain text, including its formatting.
        </p>
      ) : null}
      {action.recipients?.length ? (
        <p className="break-all text-muted-foreground">To: {action.recipients.join(", ")}</p>
      ) : null}
      {action.app === "mail" && action.operation === "create" ? (
        <p className="text-muted-foreground">Saves a draft in Mail. Heiwa never sends mail.</p>
      ) : null}
      {action.completed !== undefined ? (
        <p>
          {action.app === "mail"
            ? action.completed
              ? "Mark read"
              : "Mark unread"
            : action.completed
              ? "Mark completed"
              : "Mark incomplete"}
        </p>
      ) : null}
      <p className="break-all text-xs text-muted-foreground">
        {target
          ? `${target.title}${target.source ? ` · ${target.source}` : ""}`
          : "Target unavailable: create a fresh proposal"}
      </p>
      {action.operation === "trash" ? (
        <p className="font-medium text-destructive">
          {action.app === "notes"
            ? "Removes this note using Apple Notes. Recovery depends on its account."
            : action.app === "mail"
              ? "Moves this message to its account's Trash in Mail."
              : "Permanently removes this item from its Apple app."}
        </p>
      ) : null}
      {proposal.detail ? (
        <p role="status" className="text-destructive">
          {proposal.detail}
        </p>
      ) : null}
    </div>
  );
}

const receiptTitle = (receipt: NativeProposal) =>
  receipt.status === "applied"
    ? "Change verified in the Apple app"
    : receipt.status === "rejected"
      ? "Change rejected"
      : receipt.status === "acknowledged"
        ? "Checked outcome recorded"
        : "Change needs attention";

/** Approve, reject or acknowledge the Apple changes one environment is holding. */
export function ProposalReviewList({
  environmentId,
  proposals,
}: {
  readonly environmentId: EnvironmentId;
  readonly proposals: ReadonlyArray<NativeProposal>;
}) {
  const approve = useAtomCommand(nativeApprove);
  const resolve = useAtomCommand(nativeResolve);
  const [notes, setNotes] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState<string | null>(null);
  const [receipt, setReceipt] = useState<NativeProposal | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  // Native failures carry the next step (a macOS permission, a stale item); never drop them.
  const settle = (result: AtomCommandResult<NativeProposal, unknown>) => {
    if (result._tag === "Success") {
      setFailure(null);
      setReceipt(result.value);
      return;
    }
    const error = Cause.squash(result.cause);
    setFailure(error instanceof Error ? error.message : "The Apple app did not respond.");
  };
  // React Compiler skips functions with try/finally, so release the busy state on the promise.
  const whileBusy = (
    id: string,
    run: () => Promise<AtomCommandResult<NativeProposal, unknown>>,
  ) => {
    if (busy !== null) return;
    setBusy(id);
    void run()
      .then(settle)
      .finally(() => setBusy(null));
  };
  // One at a time, stopping at the first that is not applied: each is verified on its own.
  const approveAll = (key: string, group: ReadonlyArray<NativeProposal>) => {
    if (busy !== null) return;
    setBusy(key);
    void (async () => {
      for (const proposal of group) {
        const result = await approve({ environmentId, input: { id: proposal.id, approve: true } });
        settle(result);
        if (result._tag !== "Success" || result.value.status !== "applied") return;
      }
    })().finally(() => setBusy(null));
  };
  const disabled = busy !== null;
  return (
    <div className="space-y-4">
      {failure ? (
        <p role="alert" className="text-sm text-destructive">
          {failure}
        </p>
      ) : null}
      {groupBySource(proposals).map((group) => {
        const reviewable = group.proposals.filter((proposal) => proposal.status === "pending");
        return (
          <SettingsGroup key={group.key}>
            {group.source ? (
              <div className="flex items-center justify-between gap-4 p-4">
                <div className="min-w-0 text-sm">
                  <p className="truncate font-medium">{group.source.title}</p>
                  {group.source.sender ? (
                    <p className="mt-1 truncate text-xs text-muted-foreground">
                      From {group.source.sender}
                    </p>
                  ) : null}
                </div>
                {reviewable.length > 1 ? (
                  <Button
                    size="sm"
                    disabled={disabled}
                    onClick={() => approveAll(group.key, reviewable)}
                  >
                    Approve all {reviewable.length}
                  </Button>
                ) : null}
              </div>
            ) : null}
            {group.proposals.map((proposal) => (
              <div key={proposal.id} className="p-4">
                <ProposalContent proposal={proposal} />
                <div className="mt-4 flex items-center gap-2">
                  {proposal.status === "pending" ? (
                    <>
                      <Button
                        size="sm"
                        disabled={disabled}
                        onClick={() =>
                          whileBusy(proposal.id, () =>
                            approve({ environmentId, input: { id: proposal.id, approve: true } }),
                          )
                        }
                      >
                        Approve change
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        disabled={disabled}
                        onClick={() =>
                          whileBusy(proposal.id, () =>
                            approve({ environmentId, input: { id: proposal.id, approve: false } }),
                          )
                        }
                      >
                        Reject
                      </Button>
                    </>
                  ) : (
                    <div className="space-y-3">
                      <p className="text-sm text-muted-foreground">
                        {proposal.status === "applying"
                          ? "This change started. Inspect the native app before taking another action."
                          : "Outcome uncertain. Inspect the native app; Heiwa will not repeat this change."}
                      </p>
                      <label className="block text-sm">
                        What did you verify in the Apple app?
                        <textarea
                          className="mt-2 block w-full rounded-md border bg-transparent p-2"
                          maxLength={1000}
                          value={notes[proposal.id] ?? ""}
                          onChange={(event) =>
                            setNotes((current) => ({
                              ...current,
                              [proposal.id]: event.target.value,
                            }))
                          }
                        />
                      </label>
                      <Button
                        size="sm"
                        variant="outline"
                        disabled={disabled || !notes[proposal.id]?.trim()}
                        onClick={() =>
                          whileBusy(proposal.id, () =>
                            resolve({
                              environmentId,
                              input: { id: proposal.id, note: notes[proposal.id]!.trim() },
                            }),
                          )
                        }
                      >
                        Record checked outcome
                      </Button>
                    </div>
                  )}
                </div>
              </div>
            ))}
          </SettingsGroup>
        );
      })}
      {receipt ? (
        <div role="status" className="text-sm">
          <p className="font-medium">{receiptTitle(receipt)}</p>
          {receipt.detail ? <p className="mt-1 text-muted-foreground">{receipt.detail}</p> : null}
        </div>
      ) : null}
    </div>
  );
}

export function ProposalOutcomes({
  proposals,
}: {
  readonly proposals: ReadonlyArray<NativeProposal>;
}) {
  return (
    <div className="divide-y">
      {proposals.map((item) => (
        <details key={item.id} className="py-3 text-sm">
          <summary className="cursor-pointer">
            {item.action.title ?? item.before?.title ?? item.action.operation} · {item.status}
          </summary>
          <div className="mt-3">
            <ProposalContent proposal={item} />
          </div>
        </details>
      ))}
    </div>
  );
}
