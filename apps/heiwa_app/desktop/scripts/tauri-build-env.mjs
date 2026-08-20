import path from "node:path";

const APPLE_ARM64_LINKER_ENV = "CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER";
const MACH_O_LINKER_FLAVOR = "-C linker-flavor=ld64.lld";

export function resolveTauriBuildEnv({
  platform,
  arch,
  env,
  rustSysroot,
  pathExists,
}) {
  if (platform !== "darwin" || arch !== "arm64" || env[APPLE_ARM64_LINKER_ENV]) {
    return env;
  }

  const linker = path.join(
    rustSysroot,
    "lib",
    "rustlib",
    "aarch64-apple-darwin",
    "bin",
    "rust-lld",
  );
  if (!pathExists(linker)) {
    throw new Error(
      `Rust's bundled rust-lld is missing at ${linker}. ` +
        "Reinstall the pinned Rust toolchain before building the macOS desktop bundle.",
    );
  }

  const rustflags = env.RUSTFLAGS?.trim();
  return {
    ...env,
    [APPLE_ARM64_LINKER_ENV]: linker,
    RUSTFLAGS: rustflags
      ? `${rustflags} ${MACH_O_LINKER_FLAVOR}`
      : MACH_O_LINKER_FLAVOR,
  };
}

export function normalizeBuildArgs(args) {
  if (!args.includes("--local-app")) {
    return [...args];
  }
  if (args.filter((arg) => arg === "--local-app").length !== 1) {
    throw new Error("--local-app may be provided only once");
  }
  if (args.includes("--bundles") || args.some((arg) => arg.startsWith("--bundles="))) {
    throw new Error("--local-app already selects the app bundle");
  }

  return [
    ...args.filter((arg) => arg !== "--local-app"),
    "--bundles",
    "app",
    "--config",
    '{"bundle":{"createUpdaterArtifacts":false}}',
  ];
}

export function localRuntimeBuildPlan(repoRoot, desktopDir) {
  const targetDir = path.join(repoRoot, "target", "bundled-runtime");
  return {
    args: [
      "build",
      "--release",
      "-p",
      "heiwa-shell",
      "--bin",
      "heiwa",
      "--target-dir",
      targetDir,
    ],
    source: path.join(targetDir, "release", "heiwa"),
    target: path.join(desktopDir, "src-tauri", "resources", "heiwa"),
  };
}

export function tauriBuildInvocation(nodeExecutable, desktopDir, args) {
  return {
    command: nodeExecutable,
    args: [
      path.join(desktopDir, "node_modules", "@tauri-apps", "cli", "tauri.js"),
      "build",
      ...normalizeBuildArgs(args),
    ],
  };
}
