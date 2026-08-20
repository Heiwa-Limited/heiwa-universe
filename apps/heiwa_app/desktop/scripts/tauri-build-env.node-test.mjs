import assert from "node:assert/strict";
import test from "node:test";

// Node owns this packaging-policy suite; Vitest owns the renderer suites.

import {
  localRuntimeBuildPlan,
  normalizeBuildArgs,
  resolveTauriBuildEnv,
  tauriBuildInvocation,
} from "./tauri-build-env.mjs";

test("non-macOS builds preserve the caller environment", () => {
  const env = { PATH: "/usr/bin", RUSTFLAGS: "-C debuginfo=1" };

  assert.deepEqual(
    resolveTauriBuildEnv({
      platform: "linux",
      arch: "x64",
      env,
      rustSysroot: "/unused",
      pathExists: () => false,
    }),
    env,
  );
});

test("Apple Silicon builds use Rust's bundled Mach-O linker", () => {
  const rustSysroot = "/toolchains/1.95.0-aarch64-apple-darwin";

  assert.deepEqual(
    resolveTauriBuildEnv({
      platform: "darwin",
      arch: "arm64",
      env: { PATH: "/usr/bin", RUSTFLAGS: "-C debuginfo=1" },
      rustSysroot,
      pathExists: () => true,
    }),
    {
      PATH: "/usr/bin",
      CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER:
        `${rustSysroot}/lib/rustlib/aarch64-apple-darwin/bin/rust-lld`,
      RUSTFLAGS: "-C debuginfo=1 -C linker-flavor=ld64.lld",
    },
  );
});

test("an explicit Apple linker override remains authoritative", () => {
  const env = {
    CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER: "/operator/linker",
    RUSTFLAGS: "-C target-cpu=native",
  };

  assert.deepEqual(
    resolveTauriBuildEnv({
      platform: "darwin",
      arch: "arm64",
      env,
      rustSysroot: "/unused",
      pathExists: () => false,
    }),
    env,
  );
});

test("a missing bundled linker fails with an actionable error", () => {
  assert.throws(
    () =>
      resolveTauriBuildEnv({
        platform: "darwin",
        arch: "arm64",
        env: {},
        rustSysroot: "/toolchains/current",
        pathExists: () => false,
      }),
    /rust-lld.*reinstall the pinned Rust toolchain/is,
  );
});

test("the local app alias disables updater artifacts without changing release builds", () => {
  assert.deepEqual(normalizeBuildArgs(["--bundles", "dmg"]), ["--bundles", "dmg"]);
  assert.deepEqual(normalizeBuildArgs(["--local-app"]), [
    "--bundles",
    "app",
    "--config",
    '{"bundle":{"createUpdaterArtifacts":false}}',
  ]);
});

test("the bundled runtime cannot collide with Tauri's case-folded app output", () => {
  assert.deepEqual(localRuntimeBuildPlan("/repo", "/repo/apps/desktop"), {
    args: [
      "build",
      "--release",
      "-p",
      "heiwa-shell",
      "--bin",
      "heiwa",
      "--target-dir",
      "/repo/target/bundled-runtime",
    ],
    source: "/repo/target/bundled-runtime/release/heiwa",
    target: "/repo/apps/desktop/src-tauri/resources/heiwa",
  });
  assert.notEqual(
    localRuntimeBuildPlan("/repo", "/repo/apps/desktop").source,
    "/repo/target/release/heiwa",
  );
});

test("Tauri runs through Node without platform shell dispatch", () => {
  assert.deepEqual(
    tauriBuildInvocation("/node", "/repo/apps/desktop", ["--bundles", "dmg"]),
    {
      command: "/node",
      args: [
        "/repo/apps/desktop/node_modules/@tauri-apps/cli/tauri.js",
        "build",
        "--bundles",
        "dmg",
      ],
    },
  );
});
