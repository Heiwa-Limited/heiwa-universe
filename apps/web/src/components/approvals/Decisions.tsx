import { useAtomRefresh } from "@effect/atom-react";
import type { Decision, EnvironmentId } from "@t3tools/contracts";
import { useState } from "react";

import { decisionsAnswer, decisionsStatus } from "../../state/decisions";
import { useAtomCommand } from "../../state/use-atom-command";
import { A2uiSurface } from "../a2ui/A2uiSurface";
import type { A2uiActionEvent } from "../a2ui/a2ui.logic";

/**
 * Owner-only questions this Mac's decisions engine prepared from Messages, Mail,
 * Calendar, Reminders and holdings. Each is an A2UI surface; a button sends its
 * `answer` event back through Heiwa. Answers that would change an Apple app wait
 * for approval in the engine before anything is applied.
 */
export function DecisionList({
  environmentId,
  decisions,
}: {
  readonly environmentId: EnvironmentId;
  readonly decisions: ReadonlyArray<Decision>;
}) {
  const answer = useAtomCommand(decisionsAnswer);
  const refresh = useAtomRefresh(decisionsStatus({ environmentId, input: {} }));
  const [busy, setBusy] = useState<string | null>(null);
  const [receipt, setReceipt] = useState<{ readonly ok: boolean; readonly text: string } | null>(
    null,
  );

  const onAction = (decision: Decision, event: A2uiActionEvent) => {
    const { askId, choice, sig } = event.context;
    if (event.name !== "answer" || busy !== null) return;
    if (typeof askId !== "string" || typeof choice !== "string" || typeof sig !== "string") return;
    setBusy(decision.id);
    void answer({ environmentId, input: { askId, choice, sig } })
      .then((result) => {
        if (result._tag === "Success" && result.value.ok) {
          setReceipt({
            ok: true,
            text: `Recorded “${choice.replaceAll("_", " ")}” for ${decision.title}`,
          });
          refresh();
        } else {
          setReceipt({
            ok: false,
            text: result._tag === "Success" ? result.value.detail : "The answer could not be sent.",
          });
        }
      })
      .finally(() => setBusy(null));
  };

  return (
    <div className="space-y-3">
      {receipt ? (
        <p
          role={receipt.ok ? "status" : "alert"}
          className={receipt.ok ? "text-sm text-muted-foreground" : "text-sm text-destructive"}
        >
          {receipt.text}
        </p>
      ) : null}
      {decisions.map((decision) => (
        <A2uiSurface
          key={decision.id}
          surface={decision.surface}
          disabled={busy !== null}
          onAction={(event) => onAction(decision, event)}
        />
      ))}
    </div>
  );
}
