// @effect-diagnostics nodeBuiltinImport:off -- this provider worker supervises deadline-bound native children.
import * as NodeChildProcess from "node:child_process";
import * as NodeFSP from "node:fs/promises";
import * as NodePath from "node:path";
import * as Schema from "effect/Schema";
import type { LocalTool } from "./LocalAgent.ts";

const RelativePath = Schema.String.check(
  Schema.isMinLength(1),
  Schema.isMaxLength(4096),
  Schema.makeFilter(
    (value) =>
      !NodePath.isAbsolute(value) &&
      value.split(/[\\/]/).every((part) => part !== ".." && part !== "." && part !== "") &&
      !value.includes("\0"),
  ),
);
const ReadInput = Schema.Struct({ path: RelativePath });
const CreateInput = Schema.Struct({
  path: RelativePath,
  content: Schema.String.check(Schema.isMaxLength(64000)),
});
const ProcessInput = Schema.Struct({
  executable: Schema.String.check(Schema.makeFilter(NodePath.isAbsolute)),
  args: Schema.Array(Schema.String.check(Schema.isMaxLength(8192))).check(Schema.isMaxLength(128)),
});
const encode = Schema.encodeSync(Schema.fromJsonString(Schema.Unknown));
const FileReply = Schema.Struct({
  path: Schema.String,
  content: Schema.String,
  bytes: Schema.Number,
  sha256: Schema.String,
  created: Schema.Boolean,
});
const decodeProcessInput = Schema.decodeUnknownSync(ProcessInput);
const decodeReadInput = Schema.decodeUnknownSync(ReadInput);
const decodeCreateInput = Schema.decodeUnknownSync(CreateInput);
const decodeFileReply = Schema.decodeSync(Schema.fromJsonString(FileReply));
export const LOCAL_WORKSPACE_TOOLS: ReadonlyArray<LocalTool> = [
  {
    name: "fs_read",
    description:
      "Read a UTF-8 file inside this workspace, up to 64 KB. Symlinks and paths outside the workspace are refused. Returns content and SHA-256.",
    parameters: Schema.toJsonSchemaDocument(ReadInput).schema,
  },
  {
    name: "fs_create",
    description:
      "Create a new UTF-8 file in an existing workspace folder after host permission. Existing files are never overwritten. Returns saved content and SHA-256 readback.",
    parameters: Schema.toJsonSchemaDocument(CreateInput).schema,
  },
  {
    name: "process_run",
    description:
      "Run an absolute executable with argv after host permission. macOS sandbox: workspace read/write, system runtime reads, no network. No shell wrapping. 30-second deadline and bounded output; check exitCode and truncation before claiming success.",
    parameters: Schema.toJsonSchemaDocument(ProcessInput).schema,
  },
];
function workspaceSandbox(root: string, helper?: string) {
  const quote = (value: string) => {
    if (/[\x00-\x1f\x7f]/.test(value)) throw new Error("Invalid workspace path.");
    return encode(value);
  };
  const system = [
    "/System",
    "/usr",
    "/bin",
    "/sbin",
    "/Library/Apple",
    "/Library/Developer",
    "/private/var/db/dyld",
    "/opt/homebrew/bin",
    "/opt/homebrew/Cellar",
    "/opt/homebrew/opt",
    "/opt/homebrew/lib",
    "/opt/homebrew/share",
  ];
  return `(version 1)(deny default)(allow process-exec)(allow process-fork)(allow sysctl-read)(allow signal (target self))(allow file-read-metadata)(allow file-read* (literal "/"))${system.map((path) => `(allow file-read* (subpath ${quote(path)}))`).join("")}(allow file-read* file-write* (subpath ${quote(root)}))${helper ? `(allow file-read* (literal ${quote(helper)}))` : ""}(allow file-read* file-write* (literal "/dev/null"))(allow file-read* (literal "/dev/urandom"))`;
}
async function childRun(input: {
  command: string;
  args: string[];
  cwd: string;
  stdin?: string;
  signal?: AbortSignal;
  timeout?: number;
  outputLimit?: number;
}) {
  input.signal?.throwIfAborted();
  return await new Promise<{
    stdout: string;
    stderr: string;
    exitCode: number | null;
    timedOut: boolean;
    truncated: boolean;
  }>((resolve, reject) => {
    const child = NodeChildProcess.spawn(input.command, input.args, {
      cwd: input.cwd,
      env: {
        PATH: "/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/bin",
        HOME: input.cwd,
        TMPDIR: input.cwd,
        LANG: "en_US.UTF-8",
      },
      stdio: ["pipe", "pipe", "pipe"],
      detached: true,
    });
    let stdout = Buffer.alloc(0),
      stderr = Buffer.alloc(0),
      truncated = false,
      timedOut = false;
    const limit = input.outputLimit ?? 64000;
    const append = (old: Buffer, chunk: Buffer) => {
      if (old.length + chunk.length > limit) truncated = true;
      return Buffer.concat([old, chunk.subarray(0, Math.max(0, limit - old.length))]);
    };
    child.stdout.on("data", (chunk) => {
      stdout = append(stdout, chunk);
    });
    child.stderr.on("data", (chunk) => {
      stderr = append(stderr, chunk);
    });
    const kill = () => {
      if (child.pid) {
        try {
          process.kill(-child.pid, "SIGKILL");
        } catch {
          /* already exited */
        }
      }
    };
    const onAbort = () => kill();
    const timeoutSignal = AbortSignal.timeout(input.timeout ?? 30000);
    const onTimeout = () => {
      timedOut = true;
      kill();
    };
    timeoutSignal.addEventListener("abort", onTimeout, { once: true });
    input.signal?.addEventListener("abort", onAbort, { once: true });
    const clean = () => {
      timeoutSignal.removeEventListener("abort", onTimeout);
      input.signal?.removeEventListener("abort", onAbort);
    };
    child.once("error", (error) => {
      clean();
      reject(error);
    });
    child.once("close", (code) => {
      clean();
      kill();
      if (input.signal?.aborted) reject(input.signal.reason);
      else
        resolve({
          stdout: stdout.toString("utf8"),
          stderr: stderr.toString("utf8"),
          exitCode: code,
          timedOut,
          truncated,
        });
    });
    child.stdin.on("error", () => undefined);
    child.stdin.end(input.stdin ?? "");
  });
}
export async function makeLocalWorkspaceTools(input: {
  cwd: string;
  helper?: string;
  approve: (name: string, args: Readonly<Record<string, unknown>>) => Promise<boolean>;
}) {
  // oxlint-disable-next-line t3code/no-global-process-runtime -- Plain async factory for the standalone local agent; no Effect runtime.
  if (process.platform !== "darwin")
    throw new Error("The Heiwa local workspace tool lane currently requires macOS.");
  const root = await NodeFSP.realpath(input.cwd);
  const helper =
    input.helper ??
    NodePath.join(import.meta.dirname, "native", "heiwa-apple").replace(
      /app\.asar([/\\])/,
      "app.asar.unpacked$1",
    );
  const profile = workspaceSandbox(root, await NodeFSP.realpath(helper));
  return async (
    name: string,
    args: Readonly<Record<string, unknown>>,
    signal?: AbortSignal,
  ): Promise<unknown> => {
    signal?.throwIfAborted();
    if (name === "process_run") {
      const request = decodeProcessInput(args);
      if (!(await input.approve(name, args)))
        return { error: "Host permission denied. Nothing ran." };
      signal?.throwIfAborted();
      return childRun({
        command: "/usr/bin/sandbox-exec",
        args: ["-p", profile, request.executable, ...request.args],
        cwd: root,
        ...(signal ? { signal } : {}),
      });
    }
    const request =
      name === "fs_read"
        ? decodeReadInput(args)
        : name === "fs_create"
          ? decodeCreateInput(args)
          : null;
    if (request === null) throw new Error("No such workspace tool.");
    if (name === "fs_create" && !(await input.approve(name, args)))
      return { error: "Host permission denied. No file was created." };
    signal?.throwIfAborted();
    const result = await childRun({
      command: "/usr/bin/sandbox-exec",
      args: ["-p", profile, helper],
      cwd: root,
      stdin: encode({
        app: "files",
        operation: name === "fs_read" ? "read" : "create",
        root,
        ...request,
      }),
      timeout: 10000,
      outputLimit: 1024 * 1024,
      ...(signal ? { signal } : {}),
    });
    if (result.exitCode !== 0 || result.timedOut || result.truncated)
      throw new Error(
        "The scoped file operation was refused or incomplete. Check the path; existing files and symlinks are not replaced.",
      );
    return decodeFileReply(result.stdout);
  };
}
