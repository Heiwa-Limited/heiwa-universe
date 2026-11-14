import { useState } from "react";
import { AuthSettingsWriteScope, type NativeApp, type NativeResource } from "@t3tools/contracts";
import { useNavigate } from "@tanstack/react-router";
import type { AtomCommandResult } from "@t3tools/client-runtime/state/runtime";
import * as Cause from "effect/Cause";
import { AppleIcon, RefreshCwIcon } from "lucide-react";
import { nativeInventory, nativeSelect, nativeStatus } from "../../state/native";
import { useEnvironmentQuery } from "../../state/query";
import { useAtomCommand } from "../../state/use-atom-command";
import { readEnvironmentScope } from "../../state/session";
import { useSettingsScope } from "./SettingsScopeContext";
import { SettingsPageContainer } from "./settingsLayout";
import { SettingsGroup } from "./SettingsGroup";
import { Button } from "../ui/button";

const apps = [
  { id: "calendar", label: "Calendar", noun: "calendars", one: "calendar" },
  { id: "reminders", label: "Reminders", noun: "lists", one: "list" },
  { id: "notes", label: "Notes", noun: "folders", one: "folder" },
  { id: "mail", label: "Mail", noun: "mailboxes", one: "mailbox" },
] as const;
export function AppleAppsSettings() {
  const { environment } = useSettingsScope();
  const environmentId = environment?.environmentId;
  const canManage = !!environmentId && readEnvironmentScope(environmentId, AuthSettingsWriteScope);
  const query = useEnvironmentQuery(
    environmentId ? nativeStatus({ environmentId, input: {} }) : null,
  );
  const navigate = useNavigate();
  const inventory = useAtomCommand(nativeInventory);
  const select = useAtomCommand(nativeSelect);
  const [resources, setResources] = useState<Partial<Record<NativeApp, readonly NativeResource[]>>>(
    {},
  );
  const [busy, setBusy] = useState<string | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  // Native failures carry the next step (a macOS permission, a stale item); never drop them.
  const settle = <A, E>(result: AtomCommandResult<A, E>) => {
    if (result._tag === "Success") {
      setFailure(null);
      return result.value;
    }
    const error = Cause.squash(result.cause);
    setFailure(error instanceof Error ? error.message : "The Apple app did not respond.");
    return null;
  };
  // React Compiler skips functions with try/finally, so release the busy state on the promise.
  const whileBusy = <A,>(key: string, run: () => Promise<A>) => {
    setBusy(key);
    return run().finally(() => {
      setBusy(null);
      query.refresh();
    });
  };
  const connect = async (app: NativeApp) => {
    if (!environmentId || !canManage) return;
    const value = settle(
      await whileBusy(app, () => inventory({ environmentId, input: { app, requestAccess: true } })),
    );
    if (value !== null) setResources((current) => ({ ...current, [app]: value }));
  };
  const toggle = async (app: NativeApp, id: string) => {
    if (!environmentId || !canManage || busy !== null) return;
    const ids = query.data?.grants.find((row) => row.app === app)?.resourceIds ?? [];
    settle(
      await whileBusy(app, () =>
        select({
          environmentId,
          input: {
            app,
            resourceIds: ids.includes(id) ? ids.filter((value) => value !== id) : [...ids, id],
          },
        }),
      ),
    );
  };
  const waiting = query.data?.pending.length ?? 0;
  return (
    <SettingsPageContainer>
      <div className="mb-6 flex items-start justify-between gap-4">
        <div>
          <h2 className="flex items-center gap-2 text-lg font-semibold">
            <AppleIcon className="size-5" />
            Apple apps
          </h2>
          <p className="mt-2 max-w-lg text-sm text-muted-foreground">
            Choose the calendars, lists, folders and mailboxes Heiwa can use. Agents can read your
            selection and prepare changes. Review each change, or allow routine changes from chats
            with Full access. Removal and Notes body replacement always require review.
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
      {failure ? (
        <p role="alert" className="mb-4 text-sm text-destructive">
          {failure}
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
                      ? `${ids.length} ${ids.length === 1 ? app.one : app.noun} selected`
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
                      await whileBusy(app.id, () =>
                        select({
                          environmentId,
                          input: { app: app.id, resourceIds: ids, writeMode },
                        }),
                      );
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
      <section className="mt-8 flex items-center justify-between gap-4">
        <p className="text-sm text-muted-foreground">
          {waiting === 0
            ? "No Apple changes are waiting for review."
            : waiting === 1
              ? "1 Apple change is waiting for review."
              : `${waiting} Apple changes are waiting for review.`}
        </p>
        <Button size="sm" variant="outline" onClick={() => void navigate({ to: "/approvals" })}>
          Open approvals
        </Button>
      </section>
    </SettingsPageContainer>
  );
}
