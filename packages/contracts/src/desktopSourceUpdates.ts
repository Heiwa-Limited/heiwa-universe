import * as Schema from "effect/Schema";

export const DesktopSourceChannel = Schema.Literals(["dev", "main"]);
export type DesktopSourceChannel = typeof DesktopSourceChannel.Type;
export const DesktopSourceAction = Schema.Literals(["check", "update", "rollback"]);
export type DesktopSourceAction = typeof DesktopSourceAction.Type;
export const DesktopSourceRequest = Schema.Struct({
  action: DesktopSourceAction,
  channel: DesktopSourceChannel,
});
export type DesktopSourceRequest = typeof DesktopSourceRequest.Type;

export const DesktopSourceOperation = Schema.Struct({
  ...DesktopSourceRequest.fields,
  pid: Schema.Number,
  status: Schema.Literals(["running", "succeeded", "failed"]),
  message: Schema.String,
  logPath: Schema.String,
  targetCommit: Schema.NullOr(Schema.String),
});
export type DesktopSourceOperation = typeof DesktopSourceOperation.Type;

export const DesktopSourceState = Schema.Struct({
  supported: Schema.Boolean,
  reason: Schema.NullOr(Schema.String),
  trackedChannel: DesktopSourceChannel,
  installed: Schema.NullOr(
    Schema.Struct({
      channel: DesktopSourceChannel,
      commit: Schema.String,
      version: Schema.String,
    }),
  ),
  rollbackAvailable: Schema.Boolean,
  operation: Schema.NullOr(DesktopSourceOperation),
});
export type DesktopSourceState = typeof DesktopSourceState.Type;

/** Local installer registration; paths never come from renderer requests. */
export const DesktopSourceRegistration = Schema.Struct({
  node: Schema.String,
  script: Schema.String,
  path: Schema.String,
  app: Schema.String,
  dataDir: Schema.String,
  cacheDir: Schema.String,
  backupDir: Schema.String,
});
export type DesktopSourceRegistration = typeof DesktopSourceRegistration.Type;

/** Source-built previews update through the local installer, never the release feed. */
export const isSourcePreviewVersion = (version: string) => version.includes("-preview.");
/** Menu action that asks the renderer to open source updates and start a check. */
export const SOURCE_UPDATES_MENU_ACTION = "check-source-updates";
