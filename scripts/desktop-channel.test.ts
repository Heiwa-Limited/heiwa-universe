import { assert, describe, it } from "@effect/vitest";
import * as DateTime from "effect/DateTime";

import { isDesktopPreviewVersion, resolveDesktopUpdateChannel } from "./build-desktop-artifact.ts";
import {
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
    migrations: [[head, `M${head}`]],
  });

  it("picks the newest snapshot the build can read", () => {
    const picked = selectRestoreSnapshot(
      [
        snapshot("2026-11-01T00:00:00Z", 76),
        snapshot("2026-11-03T00:00:00Z", 78),
        snapshot("2026-11-02T00:00:00Z", 77),
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
