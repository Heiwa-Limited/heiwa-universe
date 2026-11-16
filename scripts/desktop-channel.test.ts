import * as NodeServices from "@effect/platform-node/NodeServices";
import * as Effect from "effect/Effect";
import * as FileSystem from "effect/FileSystem";
import * as Path from "effect/Path";
import { ChildProcess, ChildProcessSpawner } from "effect/process";
import { assert, describe, it } from "@effect/vitest";
import * as DateTime from "effect/DateTime";

import { isDesktopPreviewVersion, resolveDesktopUpdateChannel } from "./build-desktop-artifact.ts";
import {
  isHeiwaSource,
  channelBuildVersion,
  classifyDatabase,
  parseMigrationManifest,
  selectRestoreSnapshot,
  type Migration,
  type SnapshotRecord,
} from "./desktop-channel.ts";

const known: Migration[] = [
  [76, "HeiwaNativeWorkflows"],
  [77, "NativeWorkflowAuthority"],
];

describe("classifyDatabase", () => {
  it.each([
    { name: "no database", recorded: "missing" as const, kind: "missing" },
    { name: "unreadable database", recorded: null, kind: "unreadable" },
    { name: "same head", recorded: known, kind: "current" },
    { name: "older head", recorded: known.slice(0, 1), kind: "upgrade" },
    { name: "empty history", recorded: [], kind: "upgrade" },
    {
      name: "same id with different migration",
      recorded: [[77, "OtherBranch"]] as Migration[],
      kind: "newer",
    },
    {
      name: "missing historical migration",
      recorded: [[75, "UnknownHistory"]] as Migration[],
      kind: "newer",
    },
    {
      name: "migration the build does not know",
      recorded: [...known, [78, "FromNewerBuild"] as Migration],
      kind: "newer",
    },
  ])("$name is $kind", ({ recorded, kind }) => {
    assert.equal(classifyDatabase(recorded, known).kind, kind);
  });

  it("names the migrations a downgrade would not know", () => {
    assert.deepEqual(classifyDatabase([...known, [78, "FromNewerBuild"]], known), {
      kind: "newer",
      unknown: ["78_FromNewerBuild"],
    });
  });
});

describe("selectRestoreSnapshot", () => {
  const snapshot = (takenAt: string, head: number): SnapshotRecord => ({
    file: `/backups/${takenAt}.sqlite`,
    takenAt,
    reason: "test",
    migrations: head <= 77 ? known.filter(([id]) => id <= head) : [...known, [head, `M${head}`]],
  });

  it("picks the newest snapshot the build can read", () => {
    const picked = selectRestoreSnapshot(
      [
        snapshot("2026-11-01T00:00:00Z", 76),
        snapshot("2026-11-03T00:00:00Z", 78),
        snapshot("2026-11-02T00:00:00Z", 77),
        { ...snapshot("2026-11-04T00:00:00Z", 77), migrations: [[77, "OtherBranch"]] },
      ],
      known,
    );
    assert.equal(picked?.takenAt, "2026-11-02T00:00:00Z");
  });

  it("finds nothing when every snapshot is newer than the build", () => {
    assert.isUndefined(selectRestoreSnapshot([snapshot("2026-11-03T00:00:00Z", 78)], known));
  });
});

it("parses the migration table without the import list", () => {
  const source = [
    'import Migration0077 from "./Migrations/077_NativeWorkflowAuthority.ts";',
    "const migrationEntries = [",
    '  [1, "OrchestrationEvents", Migration0001],',
    "  // Preserve this migration's schema.",
    '  [55, "OrchestrationV2", Migration0055],',
    '  [77, "NativeWorkflowAuthority", Migration0077],',
    "] as const;",
  ].join("\n");
  assert.deepEqual(parseMigrationManifest(source), [
    [1, "OrchestrationEvents"],
    [55, "OrchestrationV2"],
    [77, "NativeWorkflowAuthority"],
  ]);
});

it("builds versions that ship without an update feed or nightly branding", () => {
  const version = channelBuildVersion({
    packageVersion: "0.4.0-preview.20261030.1",
    commitDate: DateTime.makeUnsafe("2026-11-14T23:30:00-08:00"),
    commitCount: 4321,
  });
  assert.equal(version, "0.4.0-preview.20261115.4321");
  assert.isTrue(isDesktopPreviewVersion(version));
  assert.equal(resolveDesktopUpdateChannel(version), "latest");
});

it.each([
  ["https://github.com/Heiwa-Limited/heiwa-universe.git", true],
  ["git@github.com:Heiwa-Limited/heiwa-universe.git", true],
  ["ssh://git@github.com/Heiwa-Limited/heiwa-universe", true],
  ["https://github.com/Heiwa-Limited/heiwa-universe.evil", false],
  ["https://github.com.evil.test/Heiwa-Limited/heiwa-universe.git", false],
  ["https://github.com/pingdotgg/t3code.git", false],
  ["https://github.com/Heiwa-Limited/heiwa-universe.git?redirect=evil", false],
])("validates the update authority: %s", (remote, allowed) => {
  assert.equal(isHeiwaSource(remote), allowed);
});

it.live("a second installer cannot overwrite the active operation receipt", () =>
  Effect.gen(function* () {
    const fs = yield* FileSystem.FileSystem;
    const path = yield* Path.Path;
    const spawner = yield* ChildProcessSpawner.ChildProcessSpawner;
    const directory = yield* fs.makeTempDirectoryScoped();
    yield* fs.makeDirectory(path.join(directory, ".lock"));
    const receipt = path.join(directory, "app-operation.json");
    yield* fs.writeFileString(receipt, "original operation receipt");
    const script = yield* path.fromFileUrl(new URL("./desktop-channel.ts", import.meta.url));
    const child = yield* spawner.spawn(
      ChildProcess.make(process.execPath, [
        script,
        "app-action",
        "check",
        "dev",
        "--cache-dir",
        directory,
        "--app",
        path.join(directory, "Heiwa.app"),
        "--data-dir",
        path.join(directory, "profile"),
        "--backup-dir",
        path.join(directory, "backups"),
      ]),
    );
    assert.notEqual(yield* child.exitCode, 0);
    assert.equal(yield* fs.readFileString(receipt), "original operation receipt");
  }).pipe(Effect.provide(NodeServices.layer)),
);
