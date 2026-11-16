import { useEffect, useState } from "react";
import type {
  DesktopSourceAction,
  DesktopSourceChannel,
  DesktopSourceState,
} from "@t3tools/contracts";
import { ensureLocalApi } from "../../localApi";
import { Button } from "../ui/button";
import { Select, SelectItem, SelectPopup, SelectTrigger, SelectValue } from "../ui/select";
import { searchableSetting } from "./settingsSearch";
import { SettingsRow } from "./settingsLayout";

const labels = { main: "Stable (main)", dev: "Development (dev)" };

export function SourceUpdatesSettings() {
  const bridge = window.desktopBridge?.sourceUpdates;
  const [state, setState] = useState<DesktopSourceState | null>(null);
  const [selection, setSelection] = useState<DesktopSourceChannel | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // oxlint-disable-next-line t3code/no-unreasoned-state-sync -- Poll the external installer receipt while this settings page is mounted.
  useEffect(() => {
    if (!bridge) return;
    let disposed = false;
    let timer: ReturnType<typeof setTimeout>;
    const refresh = async () => {
      try {
        const next = await bridge.getState();
        if (!disposed) setState(next);
      } catch (cause) {
        if (!disposed)
          setError(cause instanceof Error ? cause.message : "Could not read update status.");
      }
      if (!disposed) timer = setTimeout(() => void refresh(), 2_000);
    };
    void refresh();
    return () => {
      disposed = true;
      clearTimeout(timer);
    };
  }, [bridge]);

  const channel = selection ?? state?.trackedChannel ?? "dev";
  const running = pending || state?.operation?.status === "running";
  const run = async (action: DesktopSourceAction) => {
    if (!bridge || running || !state?.supported) return;
    setPending(true);
    setError(null);
    try {
      if (action !== "check") {
        const confirmed = await ensureLocalApi().dialogs.confirm(
          action === "rollback"
            ? "Restore the previous Heiwa app and restart? Incompatible databases are refused. Your followed channel stays unchanged."
            : `Update this Mac to the latest ${labels[channel]} and follow that channel? Heiwa will close and restart after the build. Finish active work first. Your database is checked and backed up before migrations.`,
        );
        if (!confirmed) {
          setPending(false);
          return;
        }
      }
      setState(await bridge.run({ action, channel }));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "The update could not start.");
    }
    setPending(false);
  };
  const installed = state?.installed;
  const operation = state?.operation;
  const upToDate =
    operation?.status === "succeeded" &&
    operation.action === "check" &&
    operation.channel === channel &&
    installed?.channel === channel &&
    operation.targetCommit === installed.commit;

  return (
    <>
      <SettingsRow
        id="app-version"
        title="Installed app"
        description={
          installed
            ? `${installed.version} · ${labels[installed.channel]} · ${installed.commit.slice(0, 12)}`
            : "Reading installed version…"
        }
      />
      <SettingsRow
        {...searchableSetting("source-update-channel")}
        description={`Currently following ${labels[state?.trackedChannel ?? "dev"]}. Selecting another channel takes effect after a successful update. Development includes changes before public promotion.`}
        control={
          <Select
            value={channel}
            onValueChange={(value) => {
              if (value === "dev" || value === "main") {
                setSelection(value);
                setError(null);
              }
            }}
          >
            <SelectTrigger
              size="sm"
              aria-label="Follow channel"
              disabled={running || !state?.supported}
            >
              <SelectValue>{labels[channel]}</SelectValue>
            </SelectTrigger>
            <SelectPopup>
              <SelectItem value="main">Stable (main)</SelectItem>
              <SelectItem value="dev">Development (dev)</SelectItem>
            </SelectPopup>
          </Select>
        }
      />
      <SettingsRow
        {...searchableSetting("source-app-updates")}
        description={
          error ??
          state?.reason ??
          (upToDate ? "This app is up to date with the selected channel." : operation?.message) ??
          "Check GitHub for the latest app on this channel. Builds run on this Mac."
        }
        control={
          <div className="flex flex-wrap gap-2">
            <Button
              size="sm"
              variant="outline"
              disabled={running || !state?.supported}
              onClick={() => void run("check")}
            >
              Check for Updates
            </Button>
            <Button
              size="sm"
              disabled={running || !state?.supported}
              onClick={() => void run("update")}
            >
              {running ? "Working…" : "Update & Restart"}
            </Button>
          </div>
        }
      />
      <SettingsRow
        {...searchableSetting("source-app-rollback")}
        description="Restore the previous installed app if its database schema is compatible. Your data is never automatically rolled back."
        control={
          <Button
            size="sm"
            variant="outline"
            disabled={running || !state?.supported || !state.rollbackAvailable}
            onClick={() => void run("rollback")}
          >
            Roll Back
          </Button>
        }
      />
    </>
  );
}
