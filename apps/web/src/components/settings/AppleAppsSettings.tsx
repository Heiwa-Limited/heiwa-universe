import { useState } from "react";
import { useAtomValue } from "@effect/atom-react";
import {
  AuthSettingsWriteScope,
  type EnvironmentId,
  LOCAL_PROVIDER_INSTANCE_ID,
  MAIL_INTAKE_PROMPT,
  MAIL_INTAKE_TASK_ID,
  type NativeApp,
  type NativeResource,
  type NativeTriageKind,
} from "@t3tools/contracts";
import { useNavigate } from "@tanstack/react-router";
import type { AtomCommandResult } from "@t3tools/client-runtime/state/runtime";
import * as Cause from "effect/Cause";
import { AppleIcon, RefreshCwIcon } from "lucide-react";
import {
  ensureScratchProject,
  nativeInventory,
  nativeSelect,
  nativeStatus,
  nativeStatusLive,
  nativeTriage,
} from "../../state/native";
import { EMPTY_SERVER_PROVIDERS, serverEnvironment } from "../../state/server";
import { useEnvironmentQuery } from "../../state/query";
import { useAtomCommand } from "../../state/use-atom-command";
import { useEnvironmentScope } from "../../state/session";
import { useSettingsScope } from "./SettingsScopeContext";
import { SettingsPageContainer } from "./settingsLayout";
import { SettingsGroup } from "./SettingsGroup";
import { Button } from "../ui/button";
import { Textarea } from "../ui/textarea";
import { ScheduledRelativeLabel, scheduleLabel } from "./ScheduledTasksSettings";

const apps = [
  { id: "calendar", label: "Calendar", noun: "calendars", one: "calendar" },
  { id: "reminders", label: "Reminders", noun: "lists", one: "list" },
  { id: "notes", label: "Notes", noun: "folders", one: "folder" },
  { id: "mail", label: "Mail", noun: "mailboxes", one: "mailbox" },
] as const;
function InboxWatch(props: {
  readonly environmentId: EnvironmentId;
  readonly busy: boolean;
  readonly hasCalendar: boolean;
  readonly run: <A, E>(command: () => Promise<AtomCommandResult<A, E>>) => Promise<A | null>;
}) {
  const { environmentId } = props;
  const tasks = useEnvironmentQuery(
    serverEnvironment.scheduledTasksLive({ environmentId, input: {} }),
  );
  // Live, so each timer check shows up while the page is open.
  const check = useEnvironmentQuery(nativeStatusLive({ environmentId, input: {} })).data?.mailWatch;
  const providers =
    useAtomValue(serverEnvironment.providersValueAtom(environmentId)) ?? EMPTY_SERVER_PROVIDERS;
  // Watching is a scheduled task, so it needs what saving one needs, not the right to grant apps.
  const canOperate = useAtomValue(
    serverEnvironment.upsertScheduledTask.permissionAtom(environmentId),
  );
  const scratch = useAtomCommand(ensureScratchProject);
  const upsertTask = useAtomCommand(serverEnvironment.upsertScheduledTask);
  const setTaskEnabled = useAtomCommand(serverEnvironment.setScheduledTaskEnabled);
  const watch = tasks.data?.tasks.find((task) => task.id === MAIL_INTAKE_TASK_ID);
  const local = providers.find(
    (provider) =>
      provider.instanceId === LOCAL_PROVIDER_INSTANCE_ID && provider.enabled && provider.installed,
  );
  const model = (local?.models.find((entry) => entry.isDefault) ?? local?.models[0])?.slug;
  // An existing watch is only paused and resumed, so edits made under Scheduled tasks survive.
  const blocked = watch
    ? null
    : !props.hasCalendar
      ? "Select a calendar first, so there is somewhere to put events."
      : !model
        ? "Connect a local model under Providers first."
        : null;
  const setWatching = async (enabled: boolean) => {
    if (watch) {
      await props.run(() => setTaskEnabled({ environmentId, input: { id: watch.id, enabled } }));
      return;
    }
    if (!model) return;
    const project = await props.run(() => scratch({ environmentId, input: {} }));
    if (project === null) return;
    await props.run(() =>
      upsertTask({
        environmentId,
        input: {
          id: MAIL_INTAKE_TASK_ID,
          title: "Mail intake",
          prompt: MAIL_INTAKE_PROMPT,
          enabled: true,
          schedule: { type: "interval", everyMs: 15 * 60_000 },
          projectId: project.projectId,
          workspaceStrategy: { type: "root" },
          modelSelection: { instanceId: LOCAL_PROVIDER_INSTANCE_ID, model },
          runtimeMode: "approval-required",
          interactionMode: "default",
          creationSource: "web",
        },
      }),
    );
  };
  return (
    <label className="mt-3 flex items-start gap-3 text-sm">
      <input
        type="checkbox"
        checked={watch?.enabled === true}
        disabled={props.busy || !canOperate || blocked !== null}
        onChange={(event) => void setWatching(event.currentTarget.checked)}
      />
      <span>
        Watch my inbox
        <span className="mt-1 block text-xs text-muted-foreground">
          {blocked ??
            "Checks for new unread mail. When some arrives, a local model reads it and prepares calendar events for you to approve."}
        </span>
        {watch ? (
          <span className="mt-1 block text-xs text-muted-foreground">
            {scheduleLabel(watch.schedule)}
            {watch.enabled ? (
              watch.nextRunAt !== null ? (
                <>
                  {" · next check "}
                  <ScheduledRelativeLabel value={watch.nextRunAt} />
                </>
              ) : null
            ) : (
              " · paused"
            )}
            {check ? (
              <>
                {" · last checked "}
                <ScheduledRelativeLabel value={check.checkedAt} />
                {check.outcome === "no-new-mail"
                  ? ", nothing new"
                  : check.outcome === "new-mail"
                    ? ", new mail found"
                    : null}
              </>
            ) : watch.enabled ? (
              " · not checked yet"
            ) : null}
          </span>
        ) : null}
        {watch && check?.outcome === "unreadable" ? (
          <span role="alert" className="mt-1 block text-xs text-destructive">
            Could not read Mail: {check.detail ?? "no detail was given."}
          </span>
        ) : null}
        {watch?.lastRunStatus === "failed" ? (
          <span role="alert" className="mt-1 block text-xs text-destructive">
            The last run failed{watch.lastRunError ? `: ${watch.lastRunError}` : "."}
          </span>
        ) : null}
      </span>
    </label>
  );
}
const triageLabels: Record<NativeTriageKind, string> = {
  meeting: "Meeting",
  "needs-clarification": "Needs clarification",
  other: "Other",
};
const TRIAGE_MAX_LENGTH = 1000;
/** Mounted per environment, so a switch drops the excerpt and any late reply. */
function MeetingTriage(props: {
  readonly environmentId: EnvironmentId;
  readonly environmentLabel: string;
}) {
  const { environmentId } = props;
  const canRun = useAtomValue(nativeTriage.permissionAtom(environmentId));
  const triage = useAtomCommand(nativeTriage);
  const [excerpt, setExcerpt] = useState("");
  const [running, setRunning] = useState(false);
  const [outcome, setOutcome] = useState<
    | { readonly kind: NativeTriageKind }
    | { readonly unavailable: string }
    | { readonly error: string }
    | null
  >(null);
  const blank = excerpt.trim().length === 0;
  const run = async () => {
    if (!canRun || running || blank) return;
    setRunning(true);
    setOutcome(null);
    const result = await triage({ environmentId, input: { excerpt } }).finally(() =>
      setRunning(false),
    );
    if (result._tag === "Success") {
      setOutcome({ kind: result.value.kind });
      return;
    }
    const error = Cause.squash(result.cause);
    const detail = error instanceof Error ? error.message : "The on-device model did not respond.";
    setOutcome(
      typeof error === "object" && error !== null && "code" in error && error.code === "unavailable"
        ? { unavailable: detail }
        : { error: detail },
    );
  };
  return (
    <section className="mt-8">
      <h3 className="text-sm font-medium">Meeting triage</h3>
      <p className="mt-1 max-w-lg text-xs text-muted-foreground">
        Paste a short excerpt to check whether it describes a meeting. {props.environmentLabel}{" "}
        receives the excerpt and Apple&apos;s on-device model processes it there. Heiwa does not
        save it or change Calendar. The label is a suggestion: wording inside an excerpt can lead to
        a wrong one.
      </p>
      <Textarea
        className="mt-3"
        size="sm"
        aria-label="Excerpt to triage"
        placeholder="Team rehearsal at 3 pm on Friday."
        maxLength={TRIAGE_MAX_LENGTH}
        value={excerpt}
        disabled={running}
        onChange={(event) => {
          setExcerpt(event.currentTarget.value);
          setOutcome(null);
        }}
      />
      <div className="mt-2 flex items-center justify-between gap-4">
        <span className="text-xs text-muted-foreground">
          {excerpt.length}/{TRIAGE_MAX_LENGTH}
        </span>
        <Button
          size="sm"
          variant="outline"
          disabled={!canRun || running || blank}
          onClick={() => void run()}
        >
          {running ? "Classifying…" : `Run on ${props.environmentLabel}`}
        </Button>
      </div>
      {outcome && "kind" in outcome ? (
        <p role="status" className="mt-2 text-sm">
          {triageLabels[outcome.kind]}
          <span className="ml-2 text-xs text-muted-foreground">on {props.environmentLabel}</span>
        </p>
      ) : outcome && "unavailable" in outcome ? (
        <p role="status" className="mt-2 text-sm text-muted-foreground">
          Unavailable on {props.environmentLabel}: {outcome.unavailable}
        </p>
      ) : outcome ? (
        <p role="alert" className="mt-2 text-sm text-destructive">
          {outcome.error}
        </p>
      ) : null}
    </section>
  );
}
export function AppleAppsSettings() {
  const { environment } = useSettingsScope();
  const environmentId = environment?.environmentId;
  const canManage = useEnvironmentScope(environmentId ?? null, AuthSettingsWriteScope);
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
  const hasCalendar =
    (query.data?.grants.find((row) => row.app === "calendar")?.resourceIds.length ?? 0) > 0;
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
              {app.id === "mail" && ids.length && environmentId ? (
                <InboxWatch
                  environmentId={environmentId}
                  busy={busy !== null}
                  hasCalendar={hasCalendar}
                  run={async (command) => settle(await whileBusy("mail", command))}
                />
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
      {environmentId && environment ? (
        <MeetingTriage
          key={environmentId}
          environmentId={environmentId}
          environmentLabel={environment.label}
        />
      ) : null}
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
