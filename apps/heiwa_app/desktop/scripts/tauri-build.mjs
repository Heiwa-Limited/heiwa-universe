import { existsSync, copyFileSync, mkdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

import {
  localRuntimeBuildPlan,
  resolveTauriBuildEnv,
  tauriBuildInvocation,
} from "./tauri-build-env.mjs";

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    env: options.env,
    stdio: options.capture ? "pipe" : "inherit",
    encoding: options.capture ? "utf8" : undefined,
    cwd: options.cwd,
  });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    const detail = options.capture ? `: ${result.stderr.trim()}` : "";
    throw new Error(`${command} exited with status ${result.status}${detail}`);
  }
  return options.capture ? result.stdout.trim() : "";
}

const rawArgs = process.argv.slice(2);
const localApp = rawArgs.includes("--local-app");
const rustSysroot =
  process.platform === "darwin" && process.arch === "arm64"
    ? run("rustc", ["--print", "sysroot"], { env: process.env, capture: true })
    : "";
const buildEnv = resolveTauriBuildEnv({
  platform: process.platform,
  arch: process.arch,
  env: process.env,
  rustSysroot,
  pathExists: existsSync,
});

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const desktopDir = path.resolve(scriptDir, "..");
const repoRoot = path.resolve(scriptDir, "../../../..");

if (localApp) {
  const runtime = localRuntimeBuildPlan(repoRoot, desktopDir);
  run("cargo", runtime.args, {
    cwd: repoRoot,
    env: buildEnv,
  });
  mkdirSync(path.dirname(runtime.target), { recursive: true });
  copyFileSync(runtime.source, runtime.target);
}

const tauri = tauriBuildInvocation(process.execPath, desktopDir, rawArgs);
run(tauri.command, tauri.args, {
  cwd: desktopDir,
  env: buildEnv,
});
