import * as NodeServices from "@effect/platform-node/NodeServices";
import { expect, it } from "@effect/vitest";
import {
  DesktopSourceOperation,
  DesktopSourceRegistration,
  DesktopSourceState,
} from "@t3tools/contracts";
import * as Effect from "effect/Effect";
import * as FileSystem from "effect/FileSystem";
import * as Path from "effect/Path";
import * as Schema from "effect/Schema";
import * as DesktopEnvironment from "../app/DesktopEnvironment.ts";
import { launchSourceUpdater, make, SourceUpdaterLaunch } from "./DesktopSourceUpdates.ts";

const setup = Effect.gen(function* () {
  const fs = yield* FileSystem.FileSystem;
  const path = yield* Path.Path;
  const home = yield* fs.makeTempDirectoryScoped();
  const cache = path.join(home, ".cache", "heiwa-desktop-channels");
  const app = path.join(home, "Applications", "Heiwa.app");
  const resourcesPath = path.join(app, "Contents", "Resources");
  const baseDir = path.join(home, ".heiwa");
  yield* fs.makeDirectory(cache, { recursive: true });
  yield* fs.makeDirectory(path.join(baseDir, "userdata"), { recursive: true });
  const registrationPath = path.join(baseDir, "userdata", "desktop-source-updater.json");
  yield* fs.makeDirectory(resourcesPath, { recursive: true });
  const write = <A, I>(file: string, schema: Schema.Codec<A, I>, value: A) =>
    Schema.encodeEffect(Schema.fromJsonString(schema))(value).pipe(
      Effect.flatMap((json) => fs.writeFileString(file, json)),
    );
  const registration: DesktopSourceRegistration = {
    app,
    cacheDir: cache,
    dataDir: baseDir,
    node: "/unused/node",
    script: "/unused/updater.ts",
    path: "/usr/bin",
    backupDir: path.join(home, "backups"),
  };
  yield* write(
    path.join(resourcesPath, "heiwa-channel.json"),
    DesktopSourceState.fields.installed,
    { channel: "dev", commit: "abc", version: "0.4.0-preview.20261115.1" },
  );
  yield* write(registrationPath, DesktopSourceRegistration, registration);
  const environment = DesktopEnvironment.DesktopEnvironment.of({
    path,
    platform: "darwin",
    isPackaged: true,
    homeDirectory: home,
    baseDir,
    resourcesPath,
  } as unknown as DesktopEnvironment.DesktopEnvironment["Service"]);
  const operation: DesktopSourceOperation = {
    action: "check",
    channel: "dev",
    pid: process.pid,
    status: "running",
    message: "Checking",
    logPath: path.join(cache, "log"),
    targetCommit: null,
  };
  const receipt = path.join(cache, "app-operation.json");
  const calls: string[] = [];
  const service = yield* make.pipe(
    Effect.provideService(DesktopEnvironment.DesktopEnvironment, environment),
    Effect.provideService(SourceUpdaterLaunch, (_registration, request) =>
      Effect.gen(function* () {
        calls.push(request.action);
        yield* write(receipt, DesktopSourceOperation, { ...operation, ...request }).pipe(
          Effect.orDie,
        );
      }),
    ),
  );
  return {
    fs,
    path,
    cache,
    registrationPath,
    environment,
    registration,
    service,
    calls,
    operation,
    receipt,
    write,
  };
});

it.layer(NodeServices.layer)("managed source updates", (it) => {
  it.effect("starts one operation, retains its receipt, and rejects concurrent requests", () =>
    Effect.gen(function* () {
      const t = yield* setup;
      expect((yield* t.service.state).supported).toBe(true);
      expect((yield* t.service.run({ action: "update", channel: "main" })).operation?.channel).toBe(
        "main",
      );
      const second = yield* Effect.result(t.service.run({ action: "rollback", channel: "dev" }));
      expect(second._tag).toBe("Failure");
      expect(t.calls).toEqual(["update"]);
      expect((yield* t.service.state).trackedChannel).toBe("dev");
      yield* t.write(t.receipt, DesktopSourceOperation, { ...t.operation, pid: -1 });
      expect((yield* t.service.state).operation?.status).toBe("failed");
      yield* t.service.run({ action: "check", channel: "dev" });
      expect(t.calls).toEqual(["update", "check"]);
    }).pipe(Effect.scoped),
  );

  it.effect("follows the installed channel until a default is chosen", () =>
    Effect.gen(function* () {
      const t = yield* setup;
      yield* t.write(
        t.path.join(t.environment.resourcesPath, "heiwa-channel.json"),
        DesktopSourceState.fields.installed,
        { channel: "main", commit: "abc", version: "0.4.0-preview.20261115.1" },
      );
      expect((yield* t.service.state).trackedChannel).toBe("main");
      yield* t.fs.writeFileString(
        t.path.join(t.cache, "state.json"),
        JSON.stringify({ defaultChannel: "dev" }),
      );
      expect((yield* t.service.state).trackedChannel).toBe("dev");
    }).pipe(Effect.scoped),
  );

  it.effect(
    "refuses a different installation or data directory and fails closed on corrupt receipts",
    () =>
      Effect.gen(function* () {
        const t = yield* setup;
        for (const wrong of [{ app: "/another/Heiwa.app" }, { dataDir: "/another/profile" }]) {
          yield* t.write(t.registrationPath, DesktopSourceRegistration, {
            ...t.registration,
            ...wrong,
          });
          expect((yield* t.service.state).supported).toBe(false);
          expect(
            (yield* Effect.result(t.service.run({ action: "update", channel: "dev" })))._tag,
          ).toBe("Failure");
        }
        yield* t.write(t.registrationPath, DesktopSourceRegistration, t.registration);
        yield* t.fs.writeFileString(t.receipt, "broken JSON");
        expect(
          (yield* Effect.result(t.service.run({ action: "update", channel: "dev" })))._tag,
        ).toBe("Failure");
        expect(t.calls).toEqual([]);
      }).pipe(Effect.scoped),
  );
});

it.live("waits for the detached helper receipt and reports startup failures", () =>
  Effect.gen(function* () {
    const t = yield* setup;
    const node = process.execPath;
    const script = t.path.join(t.cache, "helper.cjs");
    const log = t.path.join(t.cache, "helper.log");
    yield* t.fs.writeFileString(
      script,
      `require('node:fs').writeFileSync(process.argv[8] + '/ack', 'acknowledged'); process.send('ready'); process.disconnect();`,
    );
    const registration = { ...t.registration, node, script };
    // The fixture writes the acknowledgement to the supplied data-dir argument before IPC readiness.
    yield* launchSourceUpdater(registration, { action: "check", channel: "dev" }, log);
    expect(yield* t.fs.readFileString(t.path.join(registration.dataDir, "ack"))).toBe(
      "acknowledged",
    );
    yield* t.fs.writeFileString(script, "process.exit(23);");
    expect(
      (yield* Effect.result(
        launchSourceUpdater(registration, { action: "check", channel: "dev" }, log),
      ))._tag,
    ).toBe("Failure");
  }).pipe(Effect.scoped, Effect.provide(NodeServices.layer)),
);
