// @effect-diagnostics nodeBuiltinImport:off - node:sqlite has no Effect wrapper; reads and backups are one-shot.
import * as NodeSqlite from "node:sqlite";
import {
  DesktopSourceOperation,
  DesktopSourceRegistration,
  DesktopSourceAction,
} from "@t3tools/contracts";

import * as NodeRuntime from "@effect/platform-node/NodeRuntime";
import * as NodeServices from "@effect/platform-node/NodeServices";
import * as HostProcess from "@t3tools/shared/HostProcess";
import {
  PRODUCT_DATA_DIRECTORY,
  PRODUCT_ID,
  PRODUCT_INSTALL_MARKER_FILE,
} from "@t3tools/shared/product";
import * as Clock from "effect/Clock";
import * as Console from "effect/Console";
import * as DateTime from "effect/DateTime";
import * as Duration from "effect/Duration";
import * as Effect from "effect/Effect";
import * as FileSystem from "effect/FileSystem";
import * as Option from "effect/Option";
import * as Path from "effect/Path";
import * as Schema from "effect/Schema";
import * as Stream from "effect/Stream";
import { Argument, Command, Flag } from "effect/cli";
import { ChildProcess, ChildProcessSpawner } from "effect/process";
import plist from "plist";

export const CHANNELS = ["dev", "main"] as const;
export type Channel = (typeof CHANNELS)[number];
const DEFAULT_CHANNEL: Channel = "dev";
const APP_NAME = "Heiwa.app";
const SIGNING_IDENTITY = "Heiwa Local Signing";

const Migration = Schema.Tuple([Schema.Number, Schema.String]);
export type Migration = typeof Migration.Type;

const InstallMarker = Schema.Struct({
  channel: Schema.Literals(CHANNELS),
  commit: Schema.String,
  version: Schema.String,
  builtAt: Schema.String,
  migrations: Schema.Array(Migration),
});
export type InstallMarker = typeof InstallMarker.Type;
const decodeMarker = Schema.decodeUnknownEffect(Schema.fromJsonString(InstallMarker));
const encodeMarker = Schema.encodeEffect(Schema.fromJsonString(InstallMarker));

const ChannelState = Schema.Struct({
  defaultChannel: Schema.optional(Schema.Literals(CHANNELS)),
  lastInstall: Schema.optional(
    Schema.Struct({
      version: Schema.String,
      channel: Schema.optional(Schema.Literals(CHANNELS)),
      commit: Schema.optional(Schema.String),
      installedAt: Schema.String,
      source: Schema.Literals(["build", "cache", "rollback"]),
    }),
  ),
  // A build without a marker cannot say which schema it expects; remember what the
  // database held when it was displaced, keyed by its bundle version.
  unmarkedMigrations: Schema.optional(Schema.Record(Schema.String, Schema.Array(Migration))),
});
type ChannelState = typeof ChannelState.Type;
const decodeState = Schema.decodeUnknownEffect(Schema.fromJsonString(ChannelState));
const encodeState = Schema.encodeEffect(Schema.fromJsonString(ChannelState));

const SnapshotRecord = Schema.Struct({
  file: Schema.String,
  takenAt: Schema.String,
  reason: Schema.String,
  migrations: Schema.Array(Migration),
});
export type SnapshotRecord = typeof SnapshotRecord.Type;
const decodePackageVersion = Schema.decodeUnknownEffect(
  Schema.fromJsonString(Schema.Struct({ version: Schema.String })),
);
const decodeSnapshot = Schema.decodeUnknownEffect(Schema.fromJsonString(SnapshotRecord));
const encodeSnapshot = Schema.encodeEffect(Schema.fromJsonString(SnapshotRecord));

export class DesktopChannelError extends Schema.TaggedError<DesktopChannelError>()(
  "DesktopChannelError",
  { message: Schema.String },
) {}
const fail = (message: string) => Effect.fail(new DesktopChannelError({ message }));

const MIGRATION_ENTRY = /^\s*\[(\d+),\s*"([A-Za-z0-9_]+)",\s*Migration\d+\],?\s*$/gm;

/** Reads the `migrationEntries` table of apps/server/src/persistence/Migrations.ts. */
export function parseMigrationManifest(source: string): Migration[] {
  return [...source.matchAll(MIGRATION_ENTRY)].map((match) => [Number(match[1]), match[2]!]);
}

/** Keeps the `-preview.<date>.<n>` form so the build ships without an update feed. */
export function channelBuildVersion(input: {
  readonly packageVersion: string;
  readonly commitDate: DateTime.DateTime;
  readonly commitCount: number;
}): string {
  const base = input.packageVersion.split(/[-+]/)[0];
  const day = DateTime.formatIsoDateUtc(input.commitDate).replaceAll("-", "");
  return `${base}-preview.${day}.${input.commitCount}`;
}

export const migrationHead = (migrations: ReadonlyArray<Migration>) =>
  Math.max(0, ...migrations.map(([id]) => id));

/** "missing" when no database file exists; null when it exists but cannot be read. */
export type RecordedMigrations = ReadonlyArray<Migration> | "missing" | null;

export type DatabaseVerdict =
  | { readonly kind: "missing" }
  | { readonly kind: "unreadable" }
  | { readonly kind: "current"; readonly head: number }
  | { readonly kind: "upgrade"; readonly from: number; readonly to: number }
  | { readonly kind: "newer"; readonly unknown: ReadonlyArray<string> };

/** Channels must agree on every recorded migration, including historical names. */
export function classifyDatabase(
  recorded: RecordedMigrations,
  known: ReadonlyArray<Migration>,
): DatabaseVerdict {
  if (recorded === "missing") return { kind: "missing" };
  if (recorded === null) return { kind: "unreadable" };
  const knownHead = migrationHead(known);
  const names = new Map(known);
  const unknown = recorded
    .filter(([id, name]) => names.get(id) !== name)
    .map(([id, name]) => `${id}_${name}`);
  if (unknown.length > 0) return { kind: "newer", unknown };
  const recordedHead = migrationHead(recorded);
  return knownHead > recordedHead
    ? { kind: "upgrade", from: recordedHead, to: knownHead }
    : { kind: "current", head: recordedHead };
}

/** Newest snapshot whose schema the target build fully knows. */
export function selectRestoreSnapshot(
  snapshots: ReadonlyArray<SnapshotRecord>,
  known: ReadonlyArray<Migration>,
): SnapshotRecord | undefined {
  return snapshots
    .filter((snapshot) => {
      const verdict = classifyDatabase(snapshot.migrations, known);
      return verdict.kind === "current" || verdict.kind === "upgrade";
    })
    .toSorted((a, b) => b.takenAt.localeCompare(a.takenAt))[0];
}

interface Paths {
  readonly repoRoot: string;
  readonly app: string;
  readonly dataDir: string;
  readonly database: string;
  readonly cacheDir: string;
  readonly checkout: string;
  readonly builds: string;
  readonly rollbackApp: string;
  readonly statePath: string;
  readonly logs: string;
  readonly backupDir: string;
  readonly snapshots: string;
  readonly displaced: string;
}

interface PathFlags {
  readonly app: Option.Option<string>;
  readonly dataDir: Option.Option<string>;
  readonly cacheDir: Option.Option<string>;
  readonly backupDir: Option.Option<string>;
}

const resolvePaths = Effect.fn("desktopChannel.resolvePaths")(function* (flags: PathFlags) {
  const path = yield* Path.Path;
  const home = yield* HostProcess.HomeDirectory;
  const environment = yield* HostProcess.Environment;
  const repoRoot = yield* path.fromFileUrl(new URL("..", import.meta.url));
  const resolve = (value: Option.Option<string>, fallback: string) =>
    path.resolve(Option.getOrElse(value, () => fallback));
  const heiwaHome = environment.HEIWA_HOME?.trim();
  const dataDir = resolve(flags.dataDir, heiwaHome || path.join(home, PRODUCT_DATA_DIRECTORY));
  const cacheDir = resolve(flags.cacheDir, path.join(home, ".cache", "heiwa-desktop-channels"));
  const backupDir = resolve(flags.backupDir, path.join(home, "heiwa-backups", "heiwa-desktop"));
  return {
    repoRoot: path.resolve(repoRoot),
    app: resolve(flags.app, path.join(home, "Applications", APP_NAME)),
    dataDir,
    database: path.join(dataDir, "userdata", "statev2.sqlite"),
    cacheDir,
    checkout: path.join(cacheDir, "checkout"),
    builds: path.join(cacheDir, "builds"),
    rollbackApp: path.join(cacheDir, "rollback", APP_NAME),
    statePath: path.join(cacheDir, "state.json"),
    logs: path.join(cacheDir, "logs"),
    backupDir,
    snapshots: path.join(backupDir, "snapshots"),
    displaced: path.join(backupDir, "displaced"),
  } satisfies Paths;
});

const collect = <E>(stream: Stream.Stream<Uint8Array, E>) =>
  stream.pipe(
    Stream.decodeText(),
    Stream.runFold(
      () => "",
      (a, b) => a + b,
    ),
  );

const run = Effect.fn("desktopChannel.run")(
  function* (
    program: string,
    args: ReadonlyArray<string>,
    options: {
      readonly cwd?: string;
      readonly log?: string;
      readonly env?: NodeJS.ProcessEnv;
    } = {},
  ) {
    const spawner = yield* ChildProcessSpawner.ChildProcessSpawner;
    const fs = yield* FileSystem.FileSystem;
    const child = yield* spawner.spawn(
      ChildProcess.make(program, [...args], {
        ...(options.cwd ? { cwd: options.cwd } : {}),
        ...(options.env ? { env: options.env, extendEnv: false } : {}),
        stdin: "ignore",
      }),
    );
    if (options.log) {
      const log = options.log;
      yield* fs.writeFileString(log, `\n$ ${program} ${args.join(" ")}\n`, { flag: "a" });
      const append = <E>(stream: Stream.Stream<Uint8Array, E>) =>
        stream.pipe(Stream.run(fs.sink(log, { flag: "a" })));
      const [, , code] = yield* Effect.all(
        [append(child.stdout), append(child.stderr), child.exitCode],
        { concurrency: "unbounded" },
      );
      if (code === 0) return "";
      const tail = (yield* fs.readFileString(log)).trimEnd().split("\n").slice(-30).join("\n");
      return yield* fail(`${program} ${args.join(" ")} failed (${code}):\n${tail}\nLog: ${log}`);
    }
    const [stdout, stderr, code] = yield* Effect.all(
      [collect(child.stdout), collect(child.stderr), child.exitCode],
      { concurrency: "unbounded" },
    );
    if (code !== 0) {
      return yield* fail(`${program} ${args.join(" ")} failed (${code}): ${stderr.trim()}`);
    }
    return stdout.trim();
  },
  Effect.scoped,
  Effect.mapError((error) =>
    error._tag === "DesktopChannelError"
      ? error
      : new DesktopChannelError({ message: `Could not run a command: ${String(error)}` }),
  ),
);

const succeeds = (program: string, args: ReadonlyArray<string>, cwd?: string) =>
  run(program, args, cwd ? { cwd } : {}).pipe(
    Effect.as(true),
    Effect.orElseSucceed(() => false),
  );

const nowIso = Effect.map(Clock.currentTimeMillis, (millis) =>
  DateTime.formatIso(DateTime.makeUnsafe(millis)),
);
const fileStamp = Effect.map(nowIso, (iso) => iso.replaceAll(":", "-").replace(/\.\d+Z$/, "Z"));

const readJson = Effect.fn("desktopChannel.readJson")(function* <A, E, R>(
  file: string,
  decode: (raw: string) => Effect.Effect<A, E, R>,
) {
  const fs = yield* FileSystem.FileSystem;
  if (!(yield* fs.exists(file))) return Option.none<A>();
  const raw = yield* fs.readFileString(file);
  return yield* decode(raw).pipe(Effect.option);
});

const readState = (paths: Paths) =>
  readJson(paths.statePath, decodeState).pipe(
    Effect.map(Option.getOrElse((): ChannelState => ({}))),
  );

const writeState = Effect.fn("desktopChannel.writeState")(function* (
  paths: Paths,
  state: ChannelState,
) {
  const fs = yield* FileSystem.FileSystem;
  const path = yield* Path.Path;
  yield* fs.makeDirectory(path.dirname(paths.statePath), { recursive: true });
  const temporary = `${paths.statePath}.tmp`;
  yield* fs.writeFileString(temporary, `${yield* encodeState(state)}\n`);
  yield* fs.rename(temporary, paths.statePath);
});

interface BundleInfo {
  readonly path: string;
  readonly bundleId: string;
  readonly version: string;
  readonly marker: Option.Option<InstallMarker>;
}

const readBundle = Effect.fn("desktopChannel.readBundle")(function* (appPath: string) {
  const fs = yield* FileSystem.FileSystem;
  const path = yield* Path.Path;
  const infoPath = path.join(appPath, "Contents", "Info.plist");
  if (!(yield* fs.exists(infoPath))) return Option.none<BundleInfo>();
  const raw = yield* fs.readFileString(infoPath);
  const info = yield* Effect.try({
    try: () => plist.parse(raw) as Record<string, unknown>,
    catch: () => new DesktopChannelError({ message: `Could not read ${infoPath}.` }),
  });
  const marker = yield* readJson(
    path.join(appPath, "Contents", "Resources", PRODUCT_INSTALL_MARKER_FILE),
    decodeMarker,
  );
  return Option.some<BundleInfo>({
    path: appPath,
    bundleId: String(info.CFBundleIdentifier ?? ""),
    version: String(info.CFBundleShortVersionString ?? ""),
    marker,
  });
});

/** An unset default follows the installed app, so checking never silently switches channel. */
const trackedChannel = (
  state: { readonly defaultChannel?: Channel | undefined },
  installed: Option.Option<BundleInfo>,
) =>
  state.defaultChannel ??
  Option.getOrElse(
    Option.map(
      Option.flatMap(installed, (bundle) => bundle.marker),
      (marker) => marker.channel,
    ),
    () => DEFAULT_CHANNEL,
  );

const describeBundle = (bundle: BundleInfo) =>
  Option.match(bundle.marker, {
    onNone: () => `${bundle.version} (not installed by desktop-channel; channel unknown)`,
    onSome: (marker) => `${marker.channel} ${marker.commit.slice(0, 12)}  ${marker.version}`,
  });

const verifyBundle = Effect.fn("desktopChannel.verifyBundle")(function* (
  appPath: string,
  expectedVersion?: string,
) {
  const bundle = yield* readBundle(appPath);
  if (Option.isNone(bundle)) return yield* fail(`${appPath} is not an app bundle.`);
  if (bundle.value.bundleId !== PRODUCT_ID) {
    return yield* fail(`${appPath} has bundle id ${bundle.value.bundleId}, not ${PRODUCT_ID}.`);
  }
  if (expectedVersion !== undefined && bundle.value.version !== expectedVersion) {
    return yield* fail(`${appPath} reports ${bundle.value.version}, expected ${expectedVersion}.`);
  }
  yield* run("codesign", ["--verify", "--deep", "--strict", appPath]).pipe(
    Effect.mapError(
      (error) =>
        new DesktopChannelError({
          message: `${appPath} has no valid code signature. ${error.message}`,
        }),
    ),
  );
  return bundle.value;
});

/** rename, falling back to a copy when source and destination are on different volumes. */
const move = Effect.fn("desktopChannel.move")(function* (from: string, to: string) {
  const fs = yield* FileSystem.FileSystem;
  const renamed = yield* fs.rename(from, to).pipe(
    Effect.as(true),
    Effect.orElseSucceed(() => false),
  );
  if (renamed) return;
  yield* run("ditto", [from, to]);
  yield* fs.remove(from, { recursive: true });
});

function readRecordedMigrations(databasePath: string, exists: boolean): RecordedMigrations {
  if (!exists) return "missing";
  let db: NodeSqlite.DatabaseSync;
  try {
    db = new NodeSqlite.DatabaseSync(databasePath, { readOnly: true });
  } catch {
    return null;
  }
  try {
    const hasTable = db
      .prepare(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'effect_sql_migrations'",
      )
      .get();
    if (hasTable === undefined) return [];
    return db
      .prepare("SELECT migration_id, name FROM effect_sql_migrations ORDER BY migration_id")
      .all()
      .map((row): Migration => [Number(row.migration_id), String(row.name)]);
  } catch {
    return null;
  } finally {
    db.close();
  }
}

const recordedMigrations = Effect.fn("desktopChannel.recordedMigrations")(function* (
  databasePath: string,
) {
  const fs = yield* FileSystem.FileSystem;
  const exists = yield* fs.exists(databasePath);
  return readRecordedMigrations(databasePath, exists);
});

const snapshotDatabase = Effect.fn("desktopChannel.snapshotDatabase")(function* (
  paths: Paths,
  reason: string,
) {
  const fs = yield* FileSystem.FileSystem;
  const path = yield* Path.Path;
  yield* fs.makeDirectory(paths.snapshots, { recursive: true });
  const stamp = yield* fileStamp;
  const file = path.join(paths.snapshots, `statev2-${stamp}.sqlite`);
  // The SQLite backup API copies a consistent image, including committed WAL pages.
  yield* Effect.tryPromise({
    try: async () => {
      const source = new NodeSqlite.DatabaseSync(paths.database, { readOnly: true });
      try {
        await NodeSqlite.backup(source, file);
      } finally {
        source.close();
      }
    },
    catch: (cause) =>
      new DesktopChannelError({
        message: `Could not snapshot ${paths.database}: ${String(cause)}. Nothing was installed.`,
      }),
  });
  const migrations = readRecordedMigrations(file, true);
  if (migrations === null || migrations === "missing") {
    return yield* fail(`The snapshot ${file} cannot be read back. Nothing was installed.`);
  }
  const record: SnapshotRecord = { file, takenAt: yield* nowIso, reason, migrations };
  yield* fs.writeFileString(
    file.replace(/\.sqlite$/, ".json"),
    `${yield* encodeSnapshot(record)}\n`,
  );
  return record;
});

const listSnapshots = Effect.fn("desktopChannel.listSnapshots")(function* (paths: Paths) {
  const fs = yield* FileSystem.FileSystem;
  const path = yield* Path.Path;
  if (!(yield* fs.exists(paths.snapshots))) return [] as SnapshotRecord[];
  const entries = yield* fs.readDirectory(paths.snapshots);
  const records = yield* Effect.forEach(
    entries.filter((entry) => entry.endsWith(".json")),
    (entry) => readJson(path.join(paths.snapshots, entry), decodeSnapshot),
  );
  return records.flatMap((record) => Option.toArray(record));
});

const restoreSnapshot = Effect.fn("desktopChannel.restoreSnapshot")(function* (
  paths: Paths,
  known: ReadonlyArray<Migration>,
) {
  const fs = yield* FileSystem.FileSystem;
  const path = yield* Path.Path;
  const snapshot = selectRestoreSnapshot(yield* listSnapshots(paths), known);
  if (!snapshot) {
    return yield* fail(
      `No database snapshot in ${paths.snapshots} has a schema this build knows (head ${migrationHead(known)}).`,
    );
  }
  const displacedDir = path.join(paths.displaced, yield* fileStamp);
  yield* fs.makeDirectory(displacedDir, { recursive: true });
  for (const suffix of ["", "-wal", "-shm"]) {
    const file = `${paths.database}${suffix}`;
    if (yield* fs.exists(file)) yield* move(file, path.join(displacedDir, path.basename(file)));
  }
  yield* fs.copyFile(snapshot.file, paths.database);
  yield* Console.log(
    `Restored the database snapshot from ${snapshot.takenAt} (migration head ${migrationHead(snapshot.migrations)}).`,
  );
  yield* Console.log(
    `Changes made after that snapshot are not in it. The replaced database is kept in ${displacedDir}.`,
  );
});

const runningInstances = Effect.fn("desktopChannel.runningInstances")(function* () {
  const found = yield* run("lsappinfo", ["find", `bundleid=${PRODUCT_ID}`]).pipe(
    Effect.orElseSucceed(() => ""),
  );
  const asns = found.match(/ASN:0x[0-9a-f]+-0x[0-9a-f]+/gi) ?? [];
  return yield* Effect.forEach(asns, (asn) =>
    run("lsappinfo", ["info", asn]).pipe(
      Effect.map((info) => {
        const bundlePath = /bundle path="([^"]+)"/.exec(info)?.[1];
        const pid = Number(/pid = (\d+)/.exec(info)?.[1]);
        return bundlePath && Number.isInteger(pid) && pid > 0 ? [{ bundlePath, pid }] : [];
      }),
      Effect.orElseSucceed(() => []),
    ),
  ).pipe(Effect.map((rows) => rows.flat()));
});

const isAlive = (pid: number) =>
  Effect.sync(() => {
    try {
      process.kill(pid, 0);
      return true;
    } catch {
      return false;
    }
  });

/**
 * LaunchServices reports the PID that owns exactly this bundle path. Heiwa treats SIGTERM
 * like Cmd-Q (DesktopLifecycle), stopping its backend before it exits.
 */
const quitInstalledApp = Effect.fn("desktopChannel.quitInstalledApp")(function* (paths: Paths) {
  const fs = yield* FileSystem.FileSystem;
  const target = yield* fs.realPath(paths.app).pipe(Effect.orElseSucceed(() => paths.app));
  const instances = yield* runningInstances();
  for (const instance of instances) {
    const bundlePath = yield* fs
      .realPath(instance.bundlePath)
      .pipe(Effect.orElseSucceed(() => instance.bundlePath));
    if (bundlePath !== target) {
      yield* Console.log(
        `Note: another Heiwa (pid ${instance.pid}) is running from ${instance.bundlePath}; it is left alone. If it uses ${paths.dataDir}, it shares this database.`,
      );
      continue;
    }
    yield* Console.log(`Quitting the running Heiwa (pid ${instance.pid}) at ${paths.app}...`);
    yield* Effect.sync(() => process.kill(instance.pid, "SIGTERM"));
    let alive = true;
    for (let waited = 0; alive && waited < 60; waited++) {
      yield* Effect.sleep(Duration.seconds(1));
      alive = yield* isAlive(instance.pid);
    }
    if (alive) {
      return yield* fail(
        `Heiwa (pid ${instance.pid}) did not quit within 60 seconds. Quit it yourself and run this again; nothing was installed.`,
      );
    }
    yield* Console.log("Heiwa quit.");
  }
});

const git = (args: ReadonlyArray<string>, cwd: string) => run("git", args, { cwd });

interface Target {
  readonly channel: Channel;
  readonly commit: string;
  readonly version: string;
  readonly migrations: ReadonlyArray<Migration>;
}

export function isHeiwaSource(remote: string): boolean {
  return /^(?:https:\/\/github\.com\/|git@github\.com:|ssh:\/\/git@github\.com\/)Heiwa-Limited\/heiwa-universe(?:\.git)?$/.test(
    remote,
  );
}

const fetchChannel = Effect.fn("desktopChannel.fetchChannel")(function* (
  paths: Paths,
  channel: Channel,
) {
  const remote = (yield* git(["remote", "get-url", "origin"], paths.repoRoot)).trim();
  if (!isHeiwaSource(remote))
    return yield* fail(
      "Updates require origin to be the Heiwa-Limited/heiwa-universe GitHub repository.",
    );
  return yield* git(
    ["fetch", "--quiet", "origin", `+refs/heads/${channel}:refs/remotes/origin/${channel}`],
    paths.repoRoot,
  );
});

const resolveTarget = Effect.fn("desktopChannel.resolveTarget")(function* (
  paths: Paths,
  channel: Channel,
) {
  yield* fetchChannel(paths, channel);
  const commit = yield* git(
    ["rev-parse", `refs/remotes/origin/${channel}^{commit}`],
    paths.repoRoot,
  );
  const short = commit.slice(0, 12);
  if (
    !(yield* succeeds(
      "git",
      ["cat-file", "-e", `${commit}:scripts/build-desktop-artifact.ts`],
      paths.repoRoot,
    ))
  ) {
    return yield* fail(
      `origin/${channel} (${short}) has no scripts/build-desktop-artifact.ts, so it does not hold the Electron desktop app. Promote dev to ${channel} first.`,
    );
  }
  const migrations = parseMigrationManifest(
    yield* git(["show", `${commit}:apps/server/src/persistence/Migrations.ts`], paths.repoRoot),
  );
  if (migrations.length === 0) {
    return yield* fail(
      `Could not read the migration list of origin/${channel} (${short}); refusing to install a build whose schema is unknown.`,
    );
  }
  const packageJson = yield* git(["show", `${commit}:apps/server/package.json`], paths.repoRoot);
  const { version: packageVersion } = yield* decodePackageVersion(packageJson).pipe(
    Effect.mapError(
      () => new DesktopChannelError({ message: "apps/server/package.json has no version." }),
    ),
  );
  const commitDate = DateTime.make(
    yield* git(["show", "-s", "--format=%cI", commit], paths.repoRoot),
  );
  if (Option.isNone(commitDate)) return yield* fail(`Could not read the date of ${short}.`);
  const version = channelBuildVersion({
    packageVersion,
    commitDate: commitDate.value,
    commitCount: Number(yield* git(["rev-list", "--count", commit], paths.repoRoot)),
  });
  return { channel, commit, version, migrations } satisfies Target;
});

/** A detached worktree outside the main checkout, reused across builds and never on a branch. */
const prepareCheckout = Effect.fn("desktopChannel.prepareCheckout")(function* (
  paths: Paths,
  commit: string,
) {
  const fs = yield* FileSystem.FileSystem;
  const path = yield* Path.Path;
  const commonDir = (dir: string) =>
    git(["rev-parse", "--path-format=absolute", "--git-common-dir"], dir);
  if (yield* fs.exists(paths.checkout)) {
    const ours = yield* commonDir(paths.repoRoot);
    const theirs = yield* commonDir(paths.checkout).pipe(Effect.orElseSucceed(() => ""));
    if (theirs !== ours) {
      return yield* fail(
        `${paths.checkout} exists but is not a worktree of ${paths.repoRoot}. Move it aside and run this again.`,
      );
    }
  } else {
    yield* fs.makeDirectory(path.dirname(paths.checkout), { recursive: true });
    yield* git(["worktree", "add", "--force", "--detach", paths.checkout, commit], paths.repoRoot);
  }
  yield* git(["checkout", "--quiet", "--force", "--detach", commit], paths.checkout);
  // Exact tree of the commit; installed dependencies and Rust targets are kept as caches.
  yield* git(["clean", "-ffdxq", "-e", "node_modules", "-e", "/native/*/target"], paths.checkout);
});

const buildEnvironment = Effect.gen(function* () {
  const environment = yield* HostProcess.Environment;
  return Object.fromEntries(
    Object.entries(environment).filter(
      ([key]) => !/^(T3CODE_DESKTOP_|CSC_|APPLE_|HEIWA_HOME$)/.test(key),
    ),
  );
});

type Signing = { readonly identity: string } | "adhoc";

/** Decided before building so a missing identity fails in seconds, not after the build. */
const resolveSigning = Effect.fn("desktopChannel.resolveSigning")(function* (allowAdhoc: boolean) {
  const environment = yield* HostProcess.Environment;
  const identity = environment.HEIWA_LOCAL_SIGNING_IDENTITY ?? SIGNING_IDENTITY;
  const identities = yield* run("security", ["find-identity", "-p", "codesigning"]).pipe(
    Effect.orElseSucceed(() => ""),
  );
  if (identities.includes(`"${identity}"`)) return { identity } satisfies Signing;
  const consequence = `No code-signing identity named "${identity}" is in the keychain. An ad hoc signature ties macOS privacy grants (Full Disk Access, Automation, Calendars, Reminders) and keychain access to this one build, so they reset on every install.`;
  if (!allowAdhoc) {
    return yield* fail(
      `${consequence} Create the identity (see scripts/sign-macos-local.sh) or pass --allow-adhoc-signature to accept that. Nothing was built or installed.`,
    );
  }
  yield* Console.log(
    `Warning: ${consequence} Continuing because --allow-adhoc-signature was passed.`,
  );
  return "adhoc" satisfies Signing;
});

const signBundle = Effect.fn("desktopChannel.signBundle")(function* (
  paths: Paths,
  appPath: string,
  signing: Signing,
  log: string,
) {
  const path = yield* Path.Path;
  if (signing === "adhoc") {
    yield* run("codesign", ["--force", "--deep", "--sign", "-", appPath], { log });
    return;
  }
  yield* Console.log(`Signing with the local identity "${signing.identity}"...`);
  yield* run(
    "sh",
    [path.join(paths.repoRoot, "scripts", "sign-macos-local.sh"), appPath, signing.identity],
    { log },
  );
});

const findBuiltApp = Effect.fn("desktopChannel.findBuiltApp")(function* (outputDir: string) {
  const fs = yield* FileSystem.FileSystem;
  const path = yield* Path.Path;
  for (const entry of yield* fs.readDirectory(outputDir)) {
    const candidate = path.join(outputDir, entry, APP_NAME);
    if (entry.startsWith("mac") && (yield* fs.exists(candidate))) return candidate;
  }
  return yield* fail(`The build finished but ${outputDir} holds no ${APP_NAME}.`);
});

const buildTarget = Effect.fn("desktopChannel.buildTarget")(function* (
  paths: Paths,
  target: Target,
  options: { readonly signing: Signing },
) {
  const fs = yield* FileSystem.FileSystem;
  const path = yield* Path.Path;
  const node = yield* HostProcess.ExecutablePath;
  const arch = yield* HostProcess.Architecture;
  const short = target.commit.slice(0, 12);
  yield* fs.makeDirectory(paths.logs, { recursive: true });
  const log = path.join(paths.logs, `${yield* fileStamp}-${target.channel}-${short}.log`);
  yield* Console.log(`Building ${target.channel} ${short} as ${target.version}. Log: ${log}`);

  yield* prepareCheckout(paths, target.commit);
  const env = yield* buildEnvironment;
  yield* Console.log("Installing dependencies...");
  yield* run("vp", ["i", "--frozen-lockfile"], { cwd: paths.checkout, log, env });

  const outputDir = path.join(paths.builds, `.incoming-${target.channel}`);
  yield* fs.remove(outputDir, { recursive: true, force: true });
  yield* Console.log("Building the desktop app (several minutes)...");
  yield* run(
    node,
    [
      "scripts/build-desktop-artifact.ts",
      "--platform",
      "mac",
      "--target",
      "dir",
      "--arch",
      arch === "x64" ? "x64" : "arm64",
      "--build-version",
      target.version,
      "--output-dir",
      outputDir,
    ],
    { cwd: paths.checkout, log, env },
  );
  const builtApp = yield* findBuiltApp(outputDir);

  const marker: InstallMarker = {
    channel: target.channel,
    commit: target.commit,
    version: target.version,
    builtAt: yield* nowIso,
    migrations: target.migrations,
  };
  // Written before signing so the marker is sealed into the signature.
  yield* fs.writeFileString(
    path.join(builtApp, "Contents", "Resources", PRODUCT_INSTALL_MARKER_FILE),
    `${yield* encodeMarker(marker)}\n`,
  );
  yield* signBundle(paths, builtApp, options.signing, log);
  yield* verifyBundle(builtApp, target.version);

  const cached = path.join(paths.builds, target.channel, APP_NAME);
  yield* fs.remove(path.dirname(cached), { recursive: true, force: true });
  yield* fs.makeDirectory(path.dirname(cached), { recursive: true });
  yield* fs.rename(builtApp, cached);
  yield* fs.remove(outputDir, { recursive: true, force: true });
  yield* Console.log(`Built and signed ${cached}`);
  return cached;
});

const knownMigrations = (bundle: BundleInfo, state: ChannelState) =>
  Option.match(bundle.marker, {
    onSome: (marker) => marker.migrations,
    onNone: () => state.unmarkedMigrations?.[bundle.version] ?? null,
  });

const checkDatabase = Effect.fn("desktopChannel.checkDatabase")(function* (
  paths: Paths,
  known: ReadonlyArray<Migration> | null,
  label: string,
  restore: boolean,
) {
  if (known === null) {
    return yield* fail(
      `Cannot tell which database schema ${label} expects (it was not installed by desktop-channel). Refusing to install it over ${paths.database}.`,
    );
  }
  const verdict = classifyDatabase(yield* recordedMigrations(paths.database), known);
  if (verdict.kind === "unreadable") {
    return yield* fail(
      `Cannot read ${paths.database}; refusing to install without knowing its schema.`,
    );
  }
  if (verdict.kind === "newer" && !restore) {
    return yield* fail(
      [
        `${paths.database} has migrations ${label} does not know: ${verdict.unknown.join(", ")}.`,
        "A newer build wrote them, and running an older build on this database is not safe.",
        "Stay on the newer build, wait until this channel includes those migrations, or pass --restore-db-snapshot to restore the newest snapshot this build can read (changes made after it are set aside, not deleted).",
      ].join(" "),
    );
  }
  return verdict;
});

const withLock = Effect.fn("desktopChannel.withLock")(function* <A, E, R>(
  paths: Paths,
  effect: Effect.Effect<A, E, R>,
) {
  const fs = yield* FileSystem.FileSystem;
  const path = yield* Path.Path;
  const lock = path.join(paths.cacheDir, ".lock");
  yield* fs.makeDirectory(paths.cacheDir, { recursive: true });
  return yield* Effect.acquireUseRelease(
    fs.makeDirectory(lock).pipe(
      Effect.mapError(
        () =>
          new DesktopChannelError({
            message: `Another desktop-channel run holds ${lock}. If none is running, remove that directory.`,
          }),
      ),
    ),
    () => effect,
    () => fs.remove(lock, { recursive: true }).pipe(Effect.ignore),
  );
});

/**
 * Replaces the installed app with `source`, keeping the displaced app as the one rollback
 * generation. `consumeSource` moves the source (rollback) instead of copying it (cache).
 */
const installBundle = Effect.fn("desktopChannel.installBundle")(function* (
  paths: Paths,
  source: BundleInfo,
  options: {
    readonly consumeSource: boolean;
    readonly restoreSnapshot: boolean;
    readonly origin: "build" | "cache" | "rollback";
  },
) {
  const fs = yield* FileSystem.FileSystem;
  const path = yield* Path.Path;
  let state = yield* readState(paths);
  const label = describeBundle(source);
  const known = knownMigrations(source, state);
  const current = yield* readBundle(paths.app);
  const buildKey = (marker: InstallMarker) => `${marker.channel}@${marker.commit}`;
  const sourceKey = Option.map(source.marker, buildKey);
  const currentKey = Option.flatMap(current, (bundle) => Option.map(bundle.marker, buildKey));
  // Reinstalling the same build would push the useful rollback copy out.
  if (Option.isSome(sourceKey) && Option.contains(currentKey, sourceKey.value)) {
    yield* registerUpdater(paths);
    yield* Console.log(`${label} is already installed at ${paths.app}; nothing to do.`);
    return;
  }
  yield* checkDatabase(paths, known, label, options.restoreSnapshot);

  yield* quitInstalledApp(paths);

  const installed = yield* readBundle(paths.app);
  const recorded = yield* recordedMigrations(paths.database);
  const verdict = yield* checkDatabase(paths, known, label, options.restoreSnapshot);
  if (verdict.kind === "newer") {
    yield* restoreSnapshot(paths, known ?? []);
  } else if (verdict.kind === "upgrade") {
    const snapshot = yield* snapshotDatabase(
      paths,
      `before ${label} migrates the database from ${verdict.from} to ${verdict.to}`,
    );
    yield* Console.log(`Snapshot of the database before migration: ${snapshot.file}`);
  } else if (
    verdict.kind === "current" &&
    Option.isSome(installed) &&
    Option.isNone(installed.value.marker)
  ) {
    const snapshot = yield* snapshotDatabase(
      paths,
      `before the first desktop-channel install over ${installed.value.version}`,
    );
    yield* Console.log(
      `Snapshot of the database before the first managed install: ${snapshot.file}`,
    );
  }

  const parent = path.dirname(paths.app);
  const incoming = path.join(parent, `.${APP_NAME}.incoming`);
  const outgoing = path.join(parent, `.${APP_NAME}.outgoing`);
  if (yield* fs.exists(outgoing)) {
    return yield* fail(
      `${outgoing} is left over from an interrupted install and may be the only copy of the previous app. Move it aside, then run this again.`,
    );
  }
  yield* fs.makeDirectory(parent, { recursive: true });
  yield* fs.remove(incoming, { recursive: true, force: true });
  if (options.consumeSource) {
    yield* move(source.path, incoming);
  } else {
    yield* run("ditto", ["--clone", source.path, incoming]);
  }
  yield* verifyBundle(incoming, source.version).pipe(
    Effect.tapError(() =>
      options.consumeSource ? move(incoming, source.path).pipe(Effect.ignore) : Effect.void,
    ),
  );

  const hadApp = Option.isSome(installed);
  if (hadApp) yield* fs.rename(paths.app, outgoing);
  yield* fs.rename(incoming, paths.app).pipe(
    Effect.tapError(() => (hadApp ? fs.rename(outgoing, paths.app) : Effect.void)),
    Effect.mapError(
      (error) =>
        new DesktopChannelError({
          message: `Could not move the new app into place (${error.message}); the previous app was put back.`,
        }),
    ),
  );

  if (Option.isSome(installed)) {
    yield* fs.remove(paths.rollbackApp, { recursive: true, force: true });
    yield* fs.makeDirectory(path.dirname(paths.rollbackApp), { recursive: true });
    yield* move(outgoing, paths.rollbackApp);
    if (Option.isNone(installed.value.marker) && Array.isArray(recorded)) {
      state = {
        ...state,
        unmarkedMigrations: { ...state.unmarkedMigrations, [installed.value.version]: recorded },
      };
    }
  }

  const marker = Option.getOrUndefined(source.marker);
  state = {
    ...state,
    lastInstall: {
      version: source.version,
      ...(marker ? { channel: marker.channel, commit: marker.commit } : {}),
      installedAt: yield* nowIso,
      source: options.origin,
    },
  };
  yield* writeState(paths, state);
  yield* registerUpdater(paths);
  yield* Console.log(`Installed ${label} at ${paths.app}.`);
  if (Option.isSome(installed)) {
    yield* Console.log(`Rollback copy: ${describeBundle(installed.value)} at ${paths.rollbackApp}`);
  }
  yield* Console.log(`Open it with: open "${paths.app}"`);
});

const cachedBuild = Effect.fn("desktopChannel.cachedBuild")(function* (
  paths: Paths,
  channel: Channel,
) {
  const path = yield* Path.Path;
  const bundle = yield* readBundle(path.join(paths.builds, channel, APP_NAME));
  return Option.filter(bundle, (value) =>
    Option.exists(value.marker, (marker) => marker.channel === channel),
  );
});

const installChannel = Effect.fn("desktopChannel.installChannel")(function* (
  paths: Paths,
  channel: Channel,
  options: {
    readonly preferCache: boolean;
    readonly restoreSnapshot: boolean;
    readonly allowAdhoc: boolean;
  },
) {
  const cached = yield* cachedBuild(paths, channel);
  if (options.preferCache && Option.isSome(cached)) {
    yield* Console.log(`Using the cached ${channel} build (${describeBundle(cached.value)}).`);
    const source = yield* verifyBundle(cached.value.path);
    return yield* installBundle(paths, source, {
      consumeSource: false,
      restoreSnapshot: options.restoreSnapshot,
      origin: "cache",
    });
  }

  const target = yield* resolveTarget(paths, channel);
  // Fail before a long build when the database already rules this build out.
  yield* checkDatabase(
    paths,
    target.migrations,
    `${channel} ${target.commit.slice(0, 12)}`,
    options.restoreSnapshot,
  );
  const reusable = Option.filter(cached, (bundle) =>
    Option.exists(bundle.marker, (marker) => marker.commit === target.commit),
  );
  const appPath = Option.isSome(reusable)
    ? reusable.value.path
    : yield* buildTarget(paths, target, { signing: yield* resolveSigning(options.allowAdhoc) });
  if (Option.isSome(reusable)) {
    yield* Console.log(
      `origin/${channel} is still ${target.commit.slice(0, 12)}; reusing its build.`,
    );
  }
  const source = yield* verifyBundle(appPath, target.version);
  yield* installBundle(paths, source, {
    consumeSource: false,
    restoreSnapshot: options.restoreSnapshot,
    origin: Option.isSome(reusable) ? "cache" : "build",
  });
});

const remoteMovement = Effect.fn("desktopChannel.remoteMovement")(function* (
  paths: Paths,
  marker: InstallMarker,
  offline: boolean,
) {
  if (!offline) {
    const fetched = yield* fetchChannel(paths, marker.channel).pipe(
      Effect.as(true),
      Effect.orElseSucceed(() => false),
    );
    if (!fetched) return "could not fetch origin; showing the last fetched state";
  }
  const remote = yield* git(
    ["rev-parse", `refs/remotes/origin/${marker.channel}^{commit}`],
    paths.repoRoot,
  ).pipe(Effect.orElseSucceed(() => ""));
  if (remote === "") return `origin/${marker.channel} is unknown locally`;
  if (remote === marker.commit) return `up to date with origin/${marker.channel}`;
  const ahead = yield* git(
    ["rev-list", "--count", `${marker.commit}..${remote}`],
    paths.repoRoot,
  ).pipe(Effect.orElseSucceed(() => "?"));
  const ancestor = yield* succeeds(
    "git",
    ["merge-base", "--is-ancestor", marker.commit, remote],
    paths.repoRoot,
  );
  return ancestor
    ? `origin/${marker.channel} has moved ${ahead} commit(s) ahead, to ${remote.slice(0, 12)}`
    : `origin/${marker.channel} has diverged (now ${remote.slice(0, 12)})`;
});

const status = Effect.fn("desktopChannel.status")(function* (paths: Paths, offline: boolean) {
  const state = yield* readState(paths);
  const lines: string[] = [];
  const installed = yield* readBundle(paths.app);
  lines.push(`Installed  ${paths.app}`);
  if (Option.isNone(installed)) {
    lines.push("  (no app installed)");
  } else {
    const bundle = installed.value;
    if (Option.isNone(bundle.marker)) {
      lines.push(`  version   ${bundle.version}`);
      lines.push("  channel   unknown (not installed by desktop-channel)");
    } else {
      const marker = bundle.marker.value;
      const movement = yield* remoteMovement(paths, marker, offline);
      lines.push(`  channel   ${marker.channel}`);
      lines.push(`  commit    ${marker.commit.slice(0, 12)} (${movement})`);
      lines.push(`  version   ${marker.version}`);
      lines.push(`  built     ${marker.builtAt}`);
    }
    if (state.lastInstall && state.lastInstall.version === bundle.version) {
      lines.push(`  installed ${state.lastInstall.installedAt} (from ${state.lastInstall.source})`);
    }
  }
  lines.push(
    `Default    ${trackedChannel(state, installed)}${state.defaultChannel ? "" : " (unset; follows the installed app)"}`,
  );
  const rollback = yield* readBundle(paths.rollbackApp);
  lines.push(
    `Rollback   ${Option.match(rollback, { onNone: () => "none", onSome: (bundle) => `${describeBundle(bundle)} at ${paths.rollbackApp}` })}`,
  );
  for (const channel of CHANNELS) {
    const cached = yield* cachedBuild(paths, channel);
    lines.push(
      `Cached     ${channel}: ${Option.match(cached, { onNone: () => "none", onSome: describeBundle })}`,
    );
  }
  const recorded = yield* recordedMigrations(paths.database);
  const dbLine =
    recorded === "missing"
      ? "missing"
      : recorded === null
        ? "unreadable"
        : `migration head ${migrationHead(recorded)}`;
  lines.push(`Database   ${paths.database} (${dbLine})`);
  if (Option.isSome(installed)) {
    const known = knownMigrations(installed.value, state);
    if (known !== null && Array.isArray(recorded)) {
      const verdict = classifyDatabase(recorded, known);
      if (verdict.kind === "newer") {
        lines.push(`  WARNING: the installed build does not know ${verdict.unknown.join(", ")}`);
      }
    }
  }
  lines.push(`Snapshots  ${(yield* listSnapshots(paths)).length} in ${paths.snapshots}`);
  yield* Console.log(lines.join("\n"));
});

const pathFlags = {
  app: Flag.String("app").pipe(
    Flag.withDescription("App bundle to manage (default ~/Applications/Heiwa.app)."),
    Flag.optional,
  ),
  dataDir: Flag.String("data-dir").pipe(
    Flag.withDescription("Heiwa data directory (default $HEIWA_HOME or ~/.heiwa)."),
    Flag.optional,
  ),
  cacheDir: Flag.String("cache-dir").pipe(
    Flag.withDescription(
      "Build checkout, cached builds, rollback copy and state (default ~/.cache/heiwa-desktop-channels).",
    ),
    Flag.optional,
  ),
  backupDir: Flag.String("backup-dir").pipe(
    Flag.withDescription("Database snapshots (default ~/heiwa-backups/heiwa-desktop)."),
    Flag.optional,
  ),
};
const installFlags = {
  ...pathFlags,
  restoreSnapshot: Flag.Boolean("restore-db-snapshot").pipe(
    Flag.withDescription(
      "When the database is newer than the build, restore the newest snapshot it can read.",
    ),
    Flag.withDefault(false),
  ),
  allowAdhoc: Flag.Boolean("allow-adhoc-signature").pipe(
    Flag.withDescription("Sign ad hoc when the local identity is missing (privacy grants reset)."),
    Flag.withDefault(false),
  ),
};

const encodeRegistration = Schema.encodeEffect(Schema.fromJsonString(DesktopSourceRegistration));
const encodeOperation = Schema.encodeEffect(Schema.fromJsonString(DesktopSourceOperation));

const registerUpdater = Effect.fn("desktopChannel.registerUpdater")(function* (paths: Paths) {
  const fs = yield* FileSystem.FileSystem;
  const path = yield* Path.Path;
  const env = yield* HostProcess.Environment;
  const registration: DesktopSourceRegistration = {
    node: yield* HostProcess.ExecutablePath,
    script: path.join(paths.repoRoot, "scripts", "desktop-channel.ts"),
    path: env.PATH ?? "/usr/bin:/bin:/usr/sbin:/sbin",
    app: paths.app,
    dataDir: paths.dataDir,
    cacheDir: paths.cacheDir,
    backupDir: paths.backupDir,
  };
  yield* fs.makeDirectory(path.join(paths.dataDir, "userdata"), { recursive: true });
  yield* fs.writeFileString(
    path.join(paths.dataDir, "userdata", "desktop-source-updater.json"),
    yield* encodeRegistration(registration),
    { mode: 0o600 },
  );
});

const appActionCommand = Command.make(
  "app-action",
  {
    action: Argument.Literals("action", DesktopSourceAction.literals),
    channel: Argument.Literals("channel", CHANNELS),
    ...pathFlags,
  },
  (flags) =>
    Effect.gen(function* () {
      const paths = yield* resolvePaths(flags);
      const fs = yield* FileSystem.FileSystem;
      const path = yield* Path.Path;
      const receipt = path.join(paths.cacheDir, "app-operation.json");
      let started = false;
      let operation: DesktopSourceOperation = {
        action: flags.action,
        channel: flags.channel,
        pid: process.pid,
        status: "running",
        message:
          flags.action === "check"
            ? "Checking GitHub…"
            : "Preparing the app. Heiwa will restart after installation.",
        logPath: path.join(paths.cacheDir, "app-update.log"),
        targetCommit: null,
      };
      const save = () =>
        encodeOperation(operation).pipe(
          Effect.flatMap((json) => fs.writeFileString(`${receipt}.tmp`, json, { mode: 0o600 })),
          Effect.andThen(fs.rename(`${receipt}.tmp`, receipt)),
        );
      yield* withLock(
        paths,
        Effect.gen(function* () {
          yield* save();
          started = true;
          if (process.send) yield* Effect.sync(() => process.send?.("ready"));
          if (flags.action === "check") {
            const target = yield* resolveTarget(paths, flags.channel);
            yield* checkDatabase(paths, target.migrations, flags.channel, false);
            operation = {
              ...operation,
              targetCommit: target.commit,
              message: `Latest ${flags.channel}: ${target.commit.slice(0, 12)}. Ready to install.`,
            };
          } else if (flags.action === "update") {
            yield* installChannel(paths, flags.channel, {
              preferCache: false,
              restoreSnapshot: false,
              allowAdhoc: false,
            });
            const state = yield* readState(paths);
            yield* writeState(paths, { ...state, defaultChannel: flags.channel });
            operation = {
              ...operation,
              message: `Installed latest ${flags.channel}. Restarting Heiwa.`,
            };
          } else {
            const rollback = yield* readBundle(paths.rollbackApp);
            if (Option.isNone(rollback)) return yield* fail("No previous app is available.");
            yield* installBundle(paths, yield* verifyBundle(rollback.value.path), {
              consumeSource: true,
              restoreSnapshot: false,
              origin: "rollback",
            });
            operation = { ...operation, message: "Restored the previous app. Restarting Heiwa." };
          }
          if (flags.action !== "check") yield* run("open", [paths.app]);
          operation = { ...operation, status: "succeeded" };
        }),
      ).pipe(
        Effect.catch((error) =>
          Effect.sync(() => {
            operation = {
              ...operation,
              status: "failed",
              message: error instanceof Error ? error.message : String(error),
            };
          }),
        ),
      );
      if (started) yield* save();
      else yield* Console.error(operation.message);
      if (operation.status === "failed") {
        yield* Effect.sync(() => (process.exitCode = 1));
        if (started && flags.action !== "check" && (yield* fs.exists(paths.app))) {
          yield* run("open", [paths.app]).pipe(Effect.ignore);
        }
      }
    }),
);

const statusCommand = Command.make(
  "status",
  {
    ...pathFlags,
    offline: Flag.Boolean("offline").pipe(
      Flag.withDescription("Do not fetch origin."),
      Flag.withDefault(false),
    ),
  },
  (flags) => resolvePaths(flags).pipe(Effect.flatMap((paths) => status(paths, flags.offline))),
).pipe(Command.withDescription("Show the installed channel, commit and version."));

const installCommand = Command.make(
  "install",
  { channel: Argument.Literals("channel", CHANNELS).pipe(Argument.optional), ...installFlags },
  (flags) =>
    Effect.gen(function* () {
      const paths = yield* resolvePaths(flags);
      const state = yield* readState(paths);
      const tracked = trackedChannel(state, yield* readBundle(paths.app));
      const channel = Option.getOrElse(flags.channel, () => tracked);
      if (channel !== tracked) {
        yield* Console.log(`One-off install of ${channel}; this machine still tracks ${tracked}.`);
      }
      yield* withLock(
        paths,
        installChannel(paths, channel, {
          preferCache: false,
          restoreSnapshot: flags.restoreSnapshot,
          allowAdhoc: flags.allowAdhoc,
        }),
      );
    }),
).pipe(
  Command.withDescription(
    "Build the tip of origin/<channel> (default: the tracked channel) and install it.",
  ),
);

const switchCommand = Command.make(
  "switch",
  { channel: Argument.Literals("channel", CHANNELS), ...installFlags },
  (flags) =>
    Effect.gen(function* () {
      const paths = yield* resolvePaths(flags);
      yield* withLock(
        paths,
        Effect.gen(function* () {
          yield* installChannel(paths, flags.channel, {
            preferCache: true,
            restoreSnapshot: flags.restoreSnapshot,
            allowAdhoc: flags.allowAdhoc,
          });
          const state = yield* readState(paths);
          yield* writeState(paths, { ...state, defaultChannel: flags.channel });
          yield* Console.log(
            `This machine now tracks ${flags.channel}. Run install to update it to the tip of origin/${flags.channel}.`,
          );
        }),
      );
    }),
).pipe(
  Command.withDescription(
    "Track <channel> and install it, reusing its cached build when there is one.",
  ),
);

const rollbackCommand = Command.make(
  "rollback",
  { ...pathFlags, restoreSnapshot: installFlags.restoreSnapshot },
  (flags) =>
    Effect.gen(function* () {
      const paths = yield* resolvePaths(flags);
      yield* withLock(
        paths,
        Effect.gen(function* () {
          const rollback = yield* readBundle(paths.rollbackApp);
          if (Option.isNone(rollback))
            return yield* fail(`There is no rollback copy at ${paths.rollbackApp}.`);
          const source = yield* verifyBundle(rollback.value.path);
          yield* installBundle(paths, source, {
            consumeSource: true,
            restoreSnapshot: flags.restoreSnapshot,
            origin: "rollback",
          });
        }),
      );
    }),
).pipe(Command.withDescription("Swap the installed app with the rollback copy."));

const main = Command.make("desktop-channel").pipe(
  Command.withDescription(
    "Install and switch between Heiwa desktop builds of origin/dev and origin/main.",
  ),
  Command.withSubcommands([
    statusCommand,
    installCommand,
    switchCommand,
    rollbackCommand,
    appActionCommand,
  ]),
);

if (import.meta.main) {
  Command.run(main, { version: "0.0.0" }).pipe(
    Effect.catchTags({
      DesktopChannelError: (error) =>
        Console.error(error.message).pipe(
          Effect.andThen(Effect.sync(() => (process.exitCode = 1))),
        ),
    }),
    Effect.scoped,
    Effect.provide(NodeServices.layer),
    NodeRuntime.runMain,
  );
}
