import { useState } from "react";
import {
  AuthSettingsWriteScope,
  type NativeApp,
  type NativeProposal,
  type NativeResource,
} from "@t3tools/contracts";
import { AppleIcon, RefreshCwIcon } from "lucide-react";
import {
  nativeApprove,
  nativeInventory,
  nativeSelect,
  nativeStatus,
  nativeResolve,
} from "../../state/native";
import { useEnvironmentQuery } from "../../state/query";
import { useAtomCommand } from "../../state/use-atom-command";
import { readEnvironmentScope } from "../../state/session";
import { useSettingsScope } from "./SettingsScopeContext";
import { SettingsPageContainer } from "./settingsLayout";
import { SettingsGroup } from "./SettingsGroup";
import { Button } from "../ui/button";

const apps = [
  { id: "calendar", label: "Calendar", noun: "calendars" },
  { id: "reminders", label: "Reminders", noun: "lists" },
  { id: "notes", label: "Notes", noun: "folders" },
  { id: "mail", label: "Mail", noun: "mailboxes" },
] as const;
function ProposalContent({ proposal }: { proposal: NativeProposal }) {
  const action = proposal.action;
  const target = proposal.resource;
  return (
    <div className="min-w-0 space-y-2 text-sm">
      <p className="font-medium capitalize">
        {action.operation} in {action.app}
      </p>
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
      {action.start || action.end ? (
        <p className="text-muted-foreground">
          {action.start} — {action.end}
        </p>
      ) : null}
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
export function AppleAppsSettings() {
  const { environment } = useSettingsScope();
  const environmentId = environment?.environmentId;
  const canManage = !!environmentId && readEnvironmentScope(environmentId, AuthSettingsWriteScope);
  const query = useEnvironmentQuery(
    environmentId ? nativeStatus({ environmentId, input: {} }) : null,
  );
  const inventory = useAtomCommand(nativeInventory);
  const select = useAtomCommand(nativeSelect);
  const approve = useAtomCommand(nativeApprove);
  const resolve = useAtomCommand(nativeResolve);
  const [resolutionNotes, setResolutionNotes] = useState<Record<string, string>>({});
  const [resources, setResources] = useState<Partial<Record<NativeApp, readonly NativeResource[]>>>(
    {},
  );
  const [busy, setBusy] = useState<string | null>(null);
  const [receipt, setReceipt] = useState<NativeProposal | null>(null);
  const connect = async (app: NativeApp) => {
    if (!environmentId || !canManage) return;
    setBusy(app);
    try {
      const result = await inventory({ environmentId, input: { app, requestAccess: true } });
      if (result._tag === "Success")
        setResources((current) => ({ ...current, [app]: result.value }));
    } finally {
      setBusy(null);
      query.refresh();
    }
  };
  const toggle = async (app: NativeApp, id: string) => {
    if (!environmentId || !canManage || busy !== null) return;
    const ids = query.data?.grants.find((row) => row.app === app)?.resourceIds ?? [];
    setBusy(app);
    try {
      await select({
        environmentId,
        input: {
          app,
          resourceIds: ids.includes(id) ? ids.filter((value) => value !== id) : [...ids, id],
        },
      });
    } finally {
      setBusy(null);
      query.refresh();
    }
  };
  const answer = async (proposal: NativeProposal, value: boolean) => {
    if (!environmentId || !canManage || busy !== null) return;
    setBusy(proposal.id);
    try {
      const result = await approve({ environmentId, input: { id: proposal.id, approve: value } });
      if (result._tag === "Success") setReceipt(result.value);
    } finally {
      setBusy(null);
      query.refresh();
    }
  };
  return (
    <SettingsPageContainer>
      <div className="mb-6 flex items-start justify-between gap-4">
        <div>
          <h2 className="flex items-center gap-2 text-lg font-semibold">
            <AppleIcon className="size-5" />
            Apple apps
          </h2>
          <p className="mt-2 max-w-lg text-sm text-muted-foreground">
            Choose the calendars, lists, folders and mailboxes Heiwa can use. Agents can read your selection
            and prepare changes. Review each change, or allow routine changes from chats with Full
            access. Removal and Notes body replacement always require review.
          </p>
        </div>
        <Button
          size="icon-sm"
          variant="ghost"
          aria-label="Refresh Apple apps"
          onClick={query.refresh}
        >
          <RefreshCwIcon className="size-4" />
        </Button>
      </div>
      {query.error ? (
        <p role="alert" className="mb-4 text-sm text-destructive">
          {query.error}
        </p>
      ) : null}
      {query.data?.platform !== undefined && query.data.platform !== "darwin" ? (
        <p className="mb-4 text-sm text-muted-foreground">
          Connect to a Mac environment to use Apple apps.
        </p>
      ) : null}
      <SettingsGroup>
        {apps.map((app) => {
          const grant = query.data?.grants.find((row) => row.app === app.id);
          const ids = grant?.resourceIds ?? [];
          const rows = resources[app.id];
          return (
            <section key={app.id} className="p-4">
              <div className="flex items-center justify-between gap-4">
                <div>
                  <h3 className="text-sm font-medium">{app.label}</h3>
                  <p className="mt-1 text-xs text-muted-foreground">
                    {ids.length
                      ? `${ids.length} ${ids.length === 1 ? app.noun.slice(0, -1) : app.noun} selected`
                      : `No ${app.noun} selected`}
                  </p>
                </div>
                <Button
                  size="sm"
                  variant="outline"
                  disabled={!canManage || busy !== null}
                  onClick={() => void connect(app.id)}
                >
                  {busy === app.id ? "Connecting…" : rows ? "Reconnect" : "Connect"}
                </Button>
              </div>
              {ids.length ? (
                <label className="mt-3 flex items-start gap-3 text-sm">
                  <input
                    type="checkbox"
                    checked={grant?.writeMode === "automatic"}
                    disabled={!canManage || busy !== null}
                    onChange={async (event) => {
                      if (!environmentId) return;
                      const writeMode = event.currentTarget.checked ? "automatic" : "review";
                      setBusy(app.id);
                      try {
                        await select({
                          environmentId,
                          input: { app: app.id, resourceIds: ids, writeMode },
                        });
                      } finally {
                        setBusy(null);
                        query.refresh();
                      }
                    }}
                  />
                  <span>
                    Allow routine changes in selected {app.noun}
                    <span className="mt-1 block text-xs text-muted-foreground">
                      Requires Full access in the chat. Turn off to require review again.
                    </span>
                  </span>
                </label>
              ) : null}
              {rows ? (
                <div className="mt-3 space-y-2">
                  {rows.length === 0 ? (
                    <p className="text-sm text-muted-foreground">No {app.noun} available.</p>
                  ) : (
                    rows.map((resource) => (
                      <label
                        key={resource.id}
                        className="flex cursor-pointer items-center gap-3 py-1 text-sm"
                      >
                        <input
                          type="checkbox"
                          checked={ids.includes(resource.id)}
                          disabled={!canManage || busy !== null || query.isPending}
                          onChange={() => void toggle(app.id, resource.id)}
                        />
                        <span>
                          {resource.title}
                          {resource.source ? (
                            <span className="ml-2 text-xs text-muted-foreground">
                              {resource.source}
                            </span>
                          ) : null}
                        </span>
                        {!resource.writable ? (
                          <span className="text-xs text-muted-foreground">Read only</span>
                        ) : null}
                      </label>
                    ))
                  )}
                </div>
              ) : ids.length ? (
                <p className="mt-3 text-xs text-muted-foreground">
                  Reconnect to review or change your selection.
                </p>
              ) : null}
            </section>
          );
        })}
      </SettingsGroup>
      <section className="mt-8">
        <h3 className="text-sm font-semibold">Changes to review</h3>
        <p className="mt-1 text-sm text-muted-foreground">
          Approval applies only to the exact proposal shown. Changed items require a fresh proposal.
        </p>
        <div className="mt-4 space-y-4">
          {query.data?.pending.length === 0 ? (
            <p className="text-sm text-muted-foreground">No changes waiting for review.</p>
          ) : (
            query.data?.pending.map((proposal) => (
              <SettingsGroup key={proposal.id} className="p-4">
                <ProposalContent proposal={proposal} />
                <div className="mt-4 flex items-center gap-2">
                  {proposal.status === "pending" ? (
                    <>
                      <Button
                        size="sm"
                        disabled={!canManage || busy !== null}
                        onClick={() => void answer(proposal, true)}
                      >
                        Approve change
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        disabled={!canManage || busy !== null}
                        onClick={() => void answer(proposal, false)}
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
                          value={resolutionNotes[proposal.id] ?? ""}
                          onChange={(event) =>
                            setResolutionNotes((current) => ({
                              ...current,
                              [proposal.id]: event.target.value,
                            }))
                          }
                        />
                      </label>
                      <Button
                        size="sm"
                        variant="outline"
                        disabled={
                          !canManage || busy !== null || !resolutionNotes[proposal.id]?.trim()
                        }
                        onClick={async () => {
                          if (!environmentId) return;
                          setBusy(proposal.id);
                          try {
                            const result = await resolve({
                              environmentId,
                              input: {
                                id: proposal.id,
                                note: resolutionNotes[proposal.id]!.trim(),
                              },
                            });
                            if (result._tag === "Success") setReceipt(result.value);
                          } finally {
                            setBusy(null);
                            query.refresh();
                          }
                        }}
                      >
                        Record checked outcome
                      </Button>
                    </div>
                  )}
                </div>
              </SettingsGroup>
            ))
          )}
        </div>
        {receipt ? (
          <div role="status" className="mt-4 text-sm">
            <p className="font-medium">
              {receipt.status === "applied"
                ? "Change verified in the Apple app"
                : receipt.status === "rejected"
                  ? "Change rejected"
                  : receipt.status === "acknowledged"
                    ? "Checked outcome recorded"
                    : "Change needs attention"}
            </p>
            {receipt.detail ? <p className="mt-1 text-muted-foreground">{receipt.detail}</p> : null}
          </div>
        ) : null}
      </section>
      <section className="mt-8">
        <h3 className="text-sm font-semibold">Recent outcomes</h3>
        <div className="mt-3 divide-y">
          {query.data?.recent
            .filter((item) => !["pending", "applying", "uncertain"].includes(item.status))
            .map((item) => (
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
      </section>
    </SettingsPageContainer>
  );
}
