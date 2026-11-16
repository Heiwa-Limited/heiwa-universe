// @effect-diagnostics nodeBuiltinImport:off -- Detached updater must survive Electron shutdown; IPC acknowledges its durable receipt.
import * as NodeChildProcess from "node:child_process";
import * as NodeFS from "node:fs";
import {
  DesktopSourceOperation,
  DesktopSourceRegistration,
  DesktopSourceRequest,
  DesktopSourceState,
} from "@t3tools/contracts";
import { PRODUCT_INSTALL_MARKER_FILE } from "@t3tools/shared/product";
import * as Context from "effect/Context";
import * as Effect from "effect/Effect";
import * as FileSystem from "effect/FileSystem";
import * as Layer from "effect/Layer";
import * as Schema from "effect/Schema";
import * as Semaphore from "effect/Semaphore";

import * as DesktopEnvironment from "../app/DesktopEnvironment.ts";

export class SourceUpdateError extends Schema.TaggedError<SourceUpdateError>()(
  "SourceUpdateError",
  {
    message: Schema.String,
  },
) {}

export class DesktopSourceUpdates extends Context.Service<
  DesktopSourceUpdates,
  {
    readonly state: Effect.Effect<DesktopSourceState, SourceUpdateError>;
    readonly run: (
      request: DesktopSourceRequest,
    ) => Effect.Effect<DesktopSourceState, SourceUpdateError>;
  }
>()("@t3tools/desktop/updates/DesktopSourceUpdates") {}

const alive = (pid: number) => {
  if (!Number.isSafeInteger(pid) || pid <= 0) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
};

export const launchSourceUpdater = Effect.fn("DesktopSourceUpdates.launch")(function* (
  registration: DesktopSourceRegistration,
  request: DesktopSourceRequest,
  logPath: string,
) {
  yield* Effect.callback<void, SourceUpdateError>((resume) => {
    let child: NodeChildProcess.ChildProcess;
    try {
      const log = NodeFS.openSync(logPath, "a", 0o600);
      try {
        child = NodeChildProcess.spawn(
          registration.node,
          [
            registration.script,
            "app-action",
            request.action,
            request.channel,
            "--app",
            registration.app,
            "--data-dir",
            registration.dataDir,
            "--cache-dir",
            registration.cacheDir,
            "--backup-dir",
            registration.backupDir,
          ],
          {
            detached: true,
            stdio: ["ignore", log, log, "ipc"],
            env: {
              PATH: registration.path,
              HOME: process.env.HOME,
              TMPDIR: process.env.TMPDIR,
              LANG: process.env.LANG,
            },
          },
        );
      } finally {
        NodeFS.closeSync(log);
      }
    } catch (error) {
      resume(Effect.fail(new SourceUpdateError({ message: String(error) })));
      return;
    }
    const failed = (message: string) => resume(Effect.fail(new SourceUpdateError({ message })));
    child.once("error", (error) => failed(error.message));
    child.once("exit", (code) =>
      failed(`Updater exited before acknowledging startup (${code}). See ${logPath}.`),
    );
    child.once("message", (message) => {
      if (message === "ready") resume(Effect.void);
    });
    return Effect.sync(() => {
      if (child.connected) child.disconnect();
      child.unref();
    });
  }).pipe(Effect.timeout("15 seconds"));
});

export const SourceUpdaterLaunch = Context.Reference<typeof launchSourceUpdater>(
  "@t3tools/desktop/updates/DesktopSourceUpdates/SourceUpdaterLaunch",
  { defaultValue: () => launchSourceUpdater },
);

/** @public */
export const make = Effect.gen(function* () {
  const environment = yield* DesktopEnvironment.DesktopEnvironment;
  const fs = yield* FileSystem.FileSystem;
  const launch = yield* SourceUpdaterLaunch;
  const path = environment.path;
  const registrationPath = path.join(
    environment.baseDir,
    "userdata",
    "desktop-source-updater.json",
  );
  const gate = yield* Semaphore.make(1);
  const read = <A, I>(file: string, schema: Schema.Codec<A, I>) =>
    fs
      .readFileString(file)
      .pipe(Effect.flatMap(Schema.decodeUnknownEffect(Schema.fromJsonString(schema))));
  const registration = read(registrationPath, DesktopSourceRegistration);
  const state = Effect.gen(function* () {
    const installed = yield* read(
      path.join(environment.resourcesPath, PRODUCT_INSTALL_MARKER_FILE),
      DesktopSourceState.fields.installed,
    ).pipe(Effect.orElseSucceed(() => null));
    const registered = yield* registration.pipe(Effect.option);
    const config = registered._tag === "Some" ? registered.value : null;
    const cache = config?.cacheDir ?? path.join(environment.baseDir, "source-updates");
    const operationPath = path.join(cache, "app-operation.json");
    const tracked = yield* read(
      path.join(cache, "state.json"),
      Schema.Struct({ defaultChannel: Schema.optional(DesktopSourceRequest.fields.channel) }),
    ).pipe(Effect.orElseSucceed(() => ({}) as { defaultChannel?: "dev" | "main" }));
    const supported = Boolean(
      config &&
      installed &&
      environment.platform === "darwin" &&
      environment.isPackaged &&
      path.resolve(config.app, "Contents", "Resources") ===
        path.resolve(environment.resourcesPath) &&
      path.resolve(config.dataDir) === path.resolve(environment.baseDir),
    );
    let operation = (yield* fs.exists(operationPath))
      ? yield* read(operationPath, DesktopSourceOperation)
      : null;
    if (operation?.status === "running" && !alive(operation.pid)) {
      operation = {
        ...operation,
        status: "failed",
        message: "The previous update was interrupted. Check for updates again before retrying.",
      };
    }
    return {
      supported,
      reason: supported
        ? null
        : "In-app source updates require a managed macOS installation. Install this build once with desktop:channel to register its updater.",
      // An unset default follows the installed app, matching desktop-channel.
      trackedChannel: tracked.defaultChannel ?? installed?.channel ?? "dev",
      installed,
      rollbackAvailable: yield* fs.exists(path.join(cache, "rollback", "Heiwa.app")),
      operation,
    } satisfies DesktopSourceState;
  }).pipe(Effect.mapError((error) => new SourceUpdateError({ message: String(error) })));

  const run = Effect.fn("DesktopSourceUpdates.run")(
    function* (request: DesktopSourceRequest) {
      return yield* gate.withPermits(1)(
        Effect.gen(function* () {
          const current = yield* state;
          if (!current.supported) return yield* new SourceUpdateError({ message: current.reason! });
          if (current.operation?.status === "running")
            return yield* new SourceUpdateError({
              message: "An update operation is already running.",
            });
          const config = yield* registration;
          yield* fs.makeDirectory(config.cacheDir, { recursive: true });
          yield* launch(config, request, path.join(config.cacheDir, "app-update.log"));
          return yield* state;
        }),
      );
    },
    Effect.mapError(
      (error) =>
        new SourceUpdateError({ message: error instanceof Error ? error.message : String(error) }),
    ),
  );
  return DesktopSourceUpdates.of({ state, run });
});
export const layer = Layer.effect(DesktopSourceUpdates, make);
